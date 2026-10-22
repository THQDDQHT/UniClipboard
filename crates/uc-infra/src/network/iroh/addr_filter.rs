//! iroh 地址候选过滤规则的唯一事实来源。
//!
//! **为什么需要这个模块**：虚拟网卡地址（Clash fake-ip 198.18/15、IPv4
//! link-local 169.254/16、CGNAT/Tailscale overlay 100.64/10 与
//! fd7a:115c:a1e0::/48）在三条路径上都得用完全相同的判定 ——
//!
//! 1. `build_addr_filter` controls endpoint address publication.
//! 2. `invitation_adapter::serialize_filtered_endpoint_ticket` 决定哪些
//!    地址进 sponsor ticket；
//! 3. `dev pairing addrs / issue --addr` 列出候选给开发者诊断。
//!
//! 任何一条路径独立维护过滤规则都会产生分叉 —— UniClipboard#486 的 Fedora
//! 配对超时就是 sponsor ticket 序列化了一个被 endpoint AddrFilter 滤掉的
//! 地址。把判定集中在这里，让上述三处共享同一份代码，从根上消除分叉的可能。

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::io;
use std::net::{IpAddr, Ipv4Addr};

use if_addrs::{IfAddr, Interface};
use iroh::address_lookup::AddrFilter;
use iroh::{EndpointAddr, TransportAddr};
use tracing::{debug, warn};

/// LAN subnets refreshed for every publication, including after network changes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LocalLanV4 {
    pub ip: Ipv4Addr,
    pub netmask: Ipv4Addr,
}

/// Exclude known container and host-only interfaces without guessing by IP range.
fn is_virtual_bridge_interface(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if name.starts_with("vethernet") {
        return matches!(
            name.as_str(),
            "vethernet (default switch)" | "vethernet (dockernat)"
        ) || name.starts_with("vethernet (wsl");
    }
    matches!(name.as_str(), "docker0" | "podman0")
        || [
            "br-",
            "veth",
            "virbr",
            "lxcbr",
            "cni",
            "flannel",
            "cali",
            "weave",
            "vmnet",
            "vmware network adapter vmnet",
            "virtualbox host-only",
            "vboxnet",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn read_interfaces() -> io::Result<Vec<Interface>> {
    if_addrs::get_if_addrs().inspect_err(|error| {
        warn!(
            target: "iroh.addr_filter",
            error_kind = ?error.kind(),
            "network interface enumeration failed; address publication suspended"
        );
    })
}

fn excluded_interface_ips(ifaces: &[Interface]) -> BTreeSet<IpAddr> {
    let usable_ips: BTreeSet<_> = ifaces
        .iter()
        .filter(|iface| !is_virtual_bridge_interface(&iface.name))
        .map(Interface::ip)
        .collect();
    ifaces
        .iter()
        .filter(|iface| is_virtual_bridge_interface(&iface.name))
        .map(Interface::ip)
        .filter(|ip| !usable_ips.contains(ip))
        .collect()
}

/// Refresh interfaces whenever Iroh republishes after a local address change.
pub(crate) fn build_addr_filter(allow_overlay: bool) -> AddrFilter {
    build_addr_filter_with_interfaces(allow_overlay, read_interfaces)
}

fn build_addr_filter_with_interfaces(
    allow_overlay: bool,
    read: impl Fn() -> io::Result<Vec<Interface>> + Send + Sync + 'static,
) -> AddrFilter {
    AddrFilter::new(move |addrs: &Vec<TransportAddr>| match read() {
        Ok(ifaces) => apply_addr_filter(
            addrs,
            allow_overlay,
            &local_lan_v4(&ifaces),
            &excluded_interface_ips(&ifaces),
        ),
        Err(_) => Cow::Owned(Vec::new()),
    })
}

/// Share interface classification between pairing mDNS and endpoint tickets.
pub(crate) fn enumerate_publish_addrs() -> Vec<IpAddr> {
    match read_interfaces() {
        Ok(ifaces) => publish_addrs(&ifaces),
        Err(_) => Vec::new(),
    }
}

/// Share current LAN multicast interfaces between pairing and peer discovery.
pub(crate) fn multicast_interfaces(addrs: &[IpAddr]) -> Vec<Ipv4Addr> {
    // The shared filter already excludes virtual bridges; omit public and overlay IPs.
    let mut interfaces: Vec<_> = addrs
        .iter()
        .filter_map(|addr| match addr {
            IpAddr::V4(ip) if ip.is_private() => Some(*ip),
            _ => None,
        })
        .collect();
    interfaces.sort_unstable();
    interfaces.dedup();
    interfaces
}

fn publish_addrs(ifaces: &[Interface]) -> Vec<IpAddr> {
    ifaces
        .iter()
        .filter(|iface| !iface.is_loopback() && !is_virtual_bridge_interface(&iface.name))
        .map(Interface::ip)
        .filter(|ip| !is_virtual_nic_ip(*ip, true))
        .collect()
}

/// 判断某个 IP 是否属于需要过滤的虚拟网卡候选。
pub(crate) fn is_virtual_nic_ip(ip: IpAddr, allow_overlay: bool) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            if (o[0] == 198 && (o[1] & 0xfe) == 18) || (o[0] == 169 && o[1] == 254) {
                return true;
            }
            if !allow_overlay && o[0] == 100 && (o[1] & 0xc0) == 64 {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            let segs = v6.segments();
            if !allow_overlay && segs[0] == 0xfd7a && segs[1] == 0x115c && segs[2] == 0xa1e0 {
                return true;
            }
            false
        }
    }
}

fn should_filter_transport_addr(
    addr: &TransportAddr,
    allow_overlay: bool,
    excluded_ips: &BTreeSet<IpAddr>,
) -> bool {
    match addr {
        TransportAddr::Ip(socket) => {
            is_virtual_nic_ip(socket.ip(), allow_overlay) || excluded_ips.contains(&socket.ip())
        }
        _ => false,
    }
}

/// True if `ip` is an IPv4 candidate that's a public, routable address
/// (excludes RFC1918 private, loopback, link-local, CGNAT, broadcast,
/// documentation, virtual-NIC ranges).
///
/// Used by the hairpin filter to identify candidates that should be dropped
/// when we know the peer is reachable via direct LAN. CGNAT/Tailscale 100.64
/// and Clash 198.18 are handled via `is_virtual_nic_ip(.., allow_overlay=false)`
/// so we re-use that judgment instead of duplicating range checks.
fn is_public_v4(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !v4.is_private()
                && !v4.is_loopback()
                && !v4.is_link_local()
                && !v4.is_broadcast()
                && !v4.is_documentation()
                && !v4.is_unspecified()
                && !is_virtual_nic_ip(IpAddr::V4(v4), /* allow_overlay= */ false)
        }
        IpAddr::V6(_) => false,
    }
}

/// True if `a` and `b` fall in the same IPv4 subnet given `netmask`.
/// Bit-wise: `(a & mask) == (b & mask)`.
fn ips_in_same_v4_subnet(a: Ipv4Addr, b: Ipv4Addr, netmask: Ipv4Addr) -> bool {
    let mask = u32::from(netmask);
    (u32::from(a) & mask) == (u32::from(b) & mask)
}

/// True if at least one peer candidate is an RFC1918 IPv4 that lives inside
/// one of our local LAN subnets — i.e. we can reach the peer directly over
/// the LAN. When this is true, the peer's public-routable IPv4 candidates
/// are *hairpin* artifacts (NAT-reflected public IP that magicsock keeps
/// trying as if it were a separate path). On most consumer routers those
/// paths either fail or succeed with much worse RTT than the LAN path,
/// causing iroh to oscillate between fast LAN (~5ms) and hairpin (~80ms+)
/// during a single transfer.
fn peer_reachable_in_local_lan(addrs: &[TransportAddr], local_lan_v4: &[LocalLanV4]) -> bool {
    if local_lan_v4.is_empty() {
        return false;
    }
    addrs.iter().any(|addr| match addr {
        TransportAddr::Ip(socket) => match socket.ip() {
            IpAddr::V4(peer_v4) if peer_v4.is_private() => local_lan_v4
                .iter()
                .any(|lan| ips_in_same_v4_subnet(peer_v4, lan.ip, lan.netmask)),
            _ => false,
        },
        _ => false,
    })
}

fn local_lan_v4(ifaces: &[Interface]) -> Vec<LocalLanV4> {
    ifaces
        .iter()
        .filter(|iface| !iface.is_loopback() && !is_virtual_bridge_interface(&iface.name))
        .filter_map(|iface| match &iface.addr {
            IfAddr::V4(v4) if v4.ip.is_private() => Some(LocalLanV4 {
                ip: v4.ip,
                netmask: v4.netmask,
            }),
            _ => None,
        })
        .collect()
}

fn log_dropped_addrs(dropped_count: usize, allow_overlay: bool) {
    if dropped_count > 0 {
        debug!(
            target: "iroh.addr_filter",
            allow_overlay,
            dropped_count,
            "filtered network address candidates",
        );
    }
}

/// 过滤 iroh `AddrFilter` 收到的候选地址集合。
///
/// 两层过滤:
///
/// 1. **虚拟 NIC 过滤** (existing) — Clash fake-ip 198.18/15、IPv4 link-local
///    169.254/16、CGNAT 100.64/10、Tailscale ULA fd7a:115c:a1e0::/48。
///    详见 `is_virtual_nic_ip`。
///
/// 2. **Hairpin public IP 过滤** (新) — 如果 peer 的某个 RFC1918 IP 跟本机
///    任一 LAN 子网同 subnet（即我们能直连 LAN），就丢掉 peer 的所有 public
///    routable IPv4 候选。这避免 iroh magicsock 在快 LAN 路径 (~5ms RTT) 和
///    NAT-hairpin-via-公网 IP 路径 (~80ms+ RTT) 之间来回 oscillate, 把一次
///    transfer 期间的 cwnd 反复打回 slow-start。 对端跨 LAN（任何 RFC1918
///    都不在我们 subnet 内）时不动 public IP, 不破坏 cross-LAN sync。
///
/// 签名形态 `&Vec<TransportAddr> -> Cow<Vec<TransportAddr>>` 由
/// `iroh::address_lookup::AddrFilter::new` 的回调契约决定 —— **不要**
/// clippy-fix 成 `&[TransportAddr]`, 否则会和 iroh 的 `Fn` 签名失配。
pub(crate) fn apply_addr_filter<'a>(
    addrs: &'a Vec<TransportAddr>,
    allow_overlay: bool,
    local_lan_v4: &[LocalLanV4],
    excluded_ips: &BTreeSet<IpAddr>,
) -> Cow<'a, Vec<TransportAddr>> {
    let any_virtual = addrs
        .iter()
        .any(|addr| should_filter_transport_addr(addr, allow_overlay, excluded_ips));
    let drop_hairpin = peer_reachable_in_local_lan(addrs, local_lan_v4);

    if !any_virtual && !drop_hairpin {
        return Cow::Borrowed(addrs);
    }

    let kept: Vec<TransportAddr> = addrs
        .iter()
        .filter(|addr| !should_filter_transport_addr(addr, allow_overlay, excluded_ips))
        .filter(|addr| {
            // Apply hairpin filter only when peer is LAN-reachable.
            if !drop_hairpin {
                return true;
            }
            match addr {
                TransportAddr::Ip(socket) => !is_public_v4(socket.ip()),
                _ => true,
            }
        })
        .cloned()
        .collect();

    log_dropped_addrs(addrs.len() - kept.len(), allow_overlay);

    Cow::Owned(kept)
}

/// Filter a complete endpoint address before serializing it into a peer ticket.
pub(crate) fn filter_endpoint_addr(addr: EndpointAddr, allow_overlay: bool) -> EndpointAddr {
    let ifaces = match read_interfaces() {
        Ok(ifaces) => ifaces,
        Err(_) => return EndpointAddr::new(addr.id),
    };
    filter_endpoint_addr_with_interfaces(addr, allow_overlay, &ifaces)
}

fn filter_endpoint_addr_with_interfaces(
    addr: EndpointAddr,
    allow_overlay: bool,
    ifaces: &[Interface],
) -> EndpointAddr {
    let excluded_ips = excluded_interface_ips(ifaces);
    let EndpointAddr { id, addrs } = addr;
    let original_count = addrs.len();
    let kept: Vec<_> = addrs
        .into_iter()
        .filter(|addr| !should_filter_transport_addr(addr, allow_overlay, &excluded_ips))
        .collect();
    log_dropped_addrs(original_count - kept.len(), allow_overlay);
    EndpointAddr::from_parts(id, kept)
}

#[cfg(test)]
mod tests {
    use super::{
        build_addr_filter_with_interfaces, filter_endpoint_addr_with_interfaces,
        is_virtual_bridge_interface, local_lan_v4, multicast_interfaces, publish_addrs,
    };
    use if_addrs::{IfAddr, Ifv4Addr, Interface};
    use iroh::address_lookup::EndpointData;
    use iroh::{EndpointAddr, SecretKey, TransportAddr};
    use std::io;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::{Arc, Mutex};

    fn interface(name: &str, ip: &str, mask: &str) -> Interface {
        Interface {
            name: name.to_owned(),
            addr: IfAddr::V4(Ifv4Addr {
                ip: ip.parse().expect("ip"),
                netmask: mask.parse().expect("mask"),
                broadcast: None,
                prefixlen: u32::from(mask.parse::<Ipv4Addr>().expect("mask")).count_ones() as u8,
            }),
            index: None,
            #[cfg(windows)]
            adapter_name: name.to_owned(),
        }
    }

    #[test]
    fn lan_subnets_exclude_container_interfaces_but_keep_physical_network() {
        for name in ["docker0", "br-123456789abc", "vEthernet (WSL)", "vmnet8"] {
            let ifaces = [
                interface("en0", "192.168.31.10", "255.255.255.0"),
                interface(name, "172.17.0.1", "255.255.0.0"),
            ];
            let subnets = local_lan_v4(&ifaces);
            assert_eq!(subnets.len(), 1, "virtual interface: {name}");
            assert_eq!(subnets[0].ip, Ipv4Addr::new(192, 168, 31, 10));
        }
    }

    #[test]
    fn physical_bridges_and_explicit_overlay_interfaces_are_not_container_bridges() {
        for name in [
            "en0",
            "en7",
            "Wi-Fi",
            "Ethernet",
            "bridge0",
            "bridge100",
            "utun3",
            "tailscale0",
            "vEthernet (External Switch)",
        ] {
            assert!(!is_virtual_bridge_interface(name), "{name}");
        }
        for name in [
            "podman0",
            "virbr0",
            "veth123",
            "vboxnet0",
            "VMware Network Adapter VMnet8",
            "vEthernet (Default Switch)",
            "vEthernet (WSL (Hyper-V firewall))",
        ] {
            assert!(is_virtual_bridge_interface(name), "{name}");
        }
    }

    #[test]
    fn pairing_and_endpoint_publication_keep_the_same_physical_addresses() {
        let ifaces = vec![
            interface("en0", "172.17.0.20", "255.255.255.0"),
            interface("docker0", "172.18.0.1", "255.255.0.0"),
            interface("utun3", "198.18.0.1", "255.255.0.0"),
        ];
        let addr = EndpointAddr::new(SecretKey::generate().public())
            .with_ip_addr("172.17.0.20:7777".parse().unwrap())
            .with_ip_addr("172.18.0.1:7777".parse().unwrap())
            .with_ip_addr("198.18.0.1:7777".parse().unwrap());
        let filtered = filter_endpoint_addr_with_interfaces(addr.clone(), false, &ifaces);
        assert_eq!(
            filtered
                .ip_addrs()
                .map(|addr| addr.ip())
                .collect::<Vec<_>>(),
            publish_addrs(&ifaces)
        );
        assert_eq!(filtered.ip_addrs().count(), 1);
        let filter = build_addr_filter_with_interfaces(false, move || Ok(ifaces.clone()));
        let data = EndpointData::new(addr.addrs.into_iter().collect());
        assert_eq!(
            data.filtered_addrs(&filter).as_ref(),
            &filtered.addrs.into_iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn address_publication_reads_changed_interfaces_without_rebuilding_the_filter() {
        let ifaces = Arc::new(Mutex::new(vec![interface(
            "en0",
            "192.168.31.10",
            "255.255.255.0",
        )]));
        let source = Arc::clone(&ifaces);
        let filter =
            build_addr_filter_with_interfaces(false, move || Ok(source.lock().unwrap().clone()));
        let data = EndpointData::new(vec![
            TransportAddr::Ip("192.168.31.10:7777".parse().unwrap()),
            TransportAddr::Ip("10.0.0.20:7777".parse().unwrap()),
        ]);
        assert_eq!(data.filtered_addrs(&filter).len(), 2);
        *ifaces.lock().unwrap() = vec![
            interface("docker0", "192.168.31.10", "255.255.255.0"),
            interface("en7", "10.0.0.20", "255.255.255.0"),
        ];
        assert_eq!(
            data.filtered_addrs(&filter).as_ref(),
            &vec![TransportAddr::Ip("10.0.0.20:7777".parse().unwrap())]
        );
    }

    #[test]
    fn shared_ip_on_a_physical_interface_remains_publishable() {
        let ifaces = vec![
            interface("docker0", "172.17.0.1", "255.255.0.0"),
            interface("en0", "172.17.0.1", "255.255.255.0"),
        ];
        let addr = EndpointAddr::new(SecretKey::generate().public())
            .with_ip_addr("172.17.0.1:7777".parse().unwrap());
        assert_eq!(
            filter_endpoint_addr_with_interfaces(addr.clone(), false, &ifaces),
            addr
        );
    }

    #[test]
    fn failed_interface_read_publishes_nothing_and_next_publication_recovers() {
        let fail = Arc::new(Mutex::new(true));
        let source = Arc::clone(&fail);
        let filter = build_addr_filter_with_interfaces(false, move || {
            if *source.lock().unwrap() {
                Err(io::Error::other("fixture"))
            } else {
                Ok(vec![interface("en0", "192.168.31.10", "255.255.255.0")])
            }
        });
        let data = EndpointData::new(vec![TransportAddr::Ip(
            "192.168.31.10:7777".parse().unwrap(),
        )]);
        assert!(data.filtered_addrs(&filter).is_empty());
        *fail.lock().unwrap() = false;
        assert_eq!(data.filtered_addrs(&filter).len(), 1);
    }

    #[test]
    fn multicast_uses_all_lan_interfaces_without_proxy_or_overlay_egress() {
        let addrs: Vec<IpAddr> = [
            "192.168.31.7",
            "10.12.135.133",
            "172.20.1.2",
            "192.168.31.7",
            "198.18.0.1",
            "105.168.4.49",
            "100.64.0.1",
            "169.254.1.2",
            "127.0.0.1",
            "::1",
            "fd7a:115c:a1e0::1",
        ]
        .into_iter()
        .map(|ip| ip.parse().expect("fixture address"))
        .collect();

        assert_eq!(
            multicast_interfaces(&addrs),
            vec![
                Ipv4Addr::new(10, 12, 135, 133),
                Ipv4Addr::new(172, 20, 1, 2),
                Ipv4Addr::new(192, 168, 31, 7),
            ]
        );
    }
}
