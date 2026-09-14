import { act, renderHook, waitFor } from '@testing-library/react'
import { createInstance } from 'i18next'
import { I18nextProvider } from 'react-i18next'
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { useSwitchSpace } from '@/hooks/useSwitchSpace'
import zhCN from '@/i18n/locales/zh-CN.json'

const switchSpace = vi.hoisted(() => vi.fn())
const snapshot = vi.hoisted(() => vi.fn())
const dispatch = vi.hoisted(() => vi.fn())
vi.mock('@/api/daemon/setupV2', async importOriginal => ({
  ...(await importOriginal<typeof import('@/api/daemon/setupV2')>()),
  switchSpace,
}))
vi.mock('@/api/daemon/device-trust', () => ({ getDeviceTrustSnapshot: snapshot }))
vi.mock('@/store/hooks', () => ({ useAppDispatch: () => dispatch }))
vi.mock('@/store/slices/devicesSlice', () => ({
  fetchLocalDeviceInfo: vi.fn(),
  fetchSpaceMembers: vi.fn(),
}))
vi.mock('@/lib/daemon-ws', () => ({
  daemonWs: { subscribe: () => () => undefined, onReconnect: () => () => undefined },
}))

const i18n = createInstance()
beforeAll(async () => {
  await i18n.init({ lng: 'zh-CN', resources: { 'zh-CN': { translation: zhCN } } })
})
beforeEach(() => vi.clearAllMocks())

describe('useSwitchSpace', () => {
  it.each(['immediate', 'pending'])(
    'explains invitation discovery failure from %s admission',
    async mode => {
      const rejection = {
        status: 'rejected',
        joinId: 'join-test',
        reason: 'invitation_unavailable',
      }
      switchSpace.mockResolvedValue(
        mode === 'immediate' ? rejection : { status: 'pending', joinId: 'join-test' }
      )
      snapshot.mockResolvedValue({ currentJoin: rejection })
      const { result } = renderHook(() => useSwitchSpace({ onOpenChange: vi.fn() }), {
        wrapper: ({ children }) => <I18nextProvider i18n={i18n}>{children}</I18nextProvider>,
      })
      act(() => {
        result.current.setCode('123456')
        result.current.setPass('test-passphrase')
      })
      await act(async () => result.current.handleSubmit())
      await waitFor(() => expect(result.current.step).toBe('failed'))
      expect(result.current.failureMessage).toContain('无法获取邀请')
      expect(result.current.failureMessage).not.toContain('内部错误')
    }
  )
})
