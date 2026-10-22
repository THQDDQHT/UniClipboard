//! Shared multicast interfaces for pairing publishers and resolvers.

use std::{net::IpAddr, time::Duration};

use crate::network::iroh::multicast_interfaces;

use swarm_discovery::{Discoverer, IpClass};

use super::discovery_constants::PAIR_SERVICE_NAME;

/// Query before the five-second discovery window expires.
const PAIR_CADENCE: Duration = Duration::from_secs(2);

pub(super) fn pairing_discoverer(actor_id: String, addrs: &[IpAddr]) -> Discoverer {
    Discoverer::new(PAIR_SERVICE_NAME.to_string(), actor_id)
        .with_ip_class(IpClass::Auto)
        .with_cadence(PAIR_CADENCE)
        .with_multicast_interfaces_v4(multicast_interfaces(addrs))
}
