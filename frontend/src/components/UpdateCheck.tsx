import { useEffect, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, Progress } from '@mantine/core'
import { IconDownload, IconExternalLink, IconFolderOpen, IconRefresh } from '@tabler/icons-react'
import {
  getAuthStatus,
  getLatestRelease,
  getUpdateProgress,
  revealUpdateInstaller,
  startUpdateDownload,
  type UpdateInfo,
} from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { notify } from '../lib/notify'

const formatSize = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

const serverMessage = (err: any): string => err?.response?.data?.error?.message ?? ''

/**
 * 桌面端的「检查更新」。
 *
 * 入口要 `desktop_update && local_console` 同时成立：安装包要落到**这台机器**的下载目录，
 * 而 `desktop_update` 只是本进程的能力位（桌面壳注册了 reveal opener），对谁都一样为真。
 * 桌面端默认监听 0.0.0.0，局域网里打开控制台的浏览器拿得到 `desktop_update: true`，
 * 但 `/api/update` 整段在 `require_local_console` 后面——只看前者就会画出一块永远只会报错的按钮。
 * 检查完全被动（不自动查、不开机查）：GitHub 未登录限流 60 次/小时/IP。
 */
export function UpdateCheck() {
  const { locale } = useAppContext()
  const queryClient = useQueryClient()
  const [enabled, setEnabled] = useState(false)
  const [info, setInfo] = useState<UpdateInfo | null>(null)
  const [checking, setChecking] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // 下载没起来、且后端进度里没记原因时（服务压根没应答）本地兜一份
  const [startError, setStartError] = useState<string | null>(null)

  useEffect(() => {
    getAuthStatus()
      .then(status => setEnabled(Boolean(status.desktop_update && status.local_console)))
      .catch(() => {})
  }, [])

  // 下载进度：真的在下人才轮询，下完/出错就停
  const progressQuery = useQuery({
    queryKey: ['update-progress'],
    queryFn: getUpdateProgress,
    enabled,
    refetchInterval: query => (query.state.data?.state === 'downloading' ? 500 : false),
  })
  const progress = progressQuery.data
  const downloading = progress?.state === 'downloading'

  const onCheck = async (force: boolean) => {
    setChecking(true)
    setError(null)
    try {
      setInfo(await getLatestRelease(force))
    } catch (err: any) {
      // 查不到要说清楚是「没查到」，不能显示成「已是最新」或一个破折号
      setInfo(null)
      setError(serverMessage(err) || err?.message || String(err))
    } finally {
      setChecking(false)
    }
  }

  const onDownload = async () => {
    setError(null)
    setStartError(null)
    try {
      const started = await startUpdateDownload()
      // 直接用 POST 的响应起头，而不是等一次 GET：轮询只在 state 已是 downloading
      // 时才续期，那一发 GET 万一失败，下载在后台跑着、界面上却什么都没有了
      queryClient.setQueryData(['update-progress'], started)
    } catch (err: any) {
      // 起头就失败时后端会把原因记进进度，面板那条注记就是它；这里兜住的是
      // 连进度都没写上的失败（服务没应答），所以取进度优先、本地兜底
      const latest = await progressQuery.refetch()
      if (latest.data?.state !== 'error') {
        setStartError(serverMessage(err) || err?.message || String(err))
      }
    }
  }

  const onReveal = async () => {
    try {
      await revealUpdateInstaller()
    } catch (err: any) {
      notify(locale, {
        color: 'red',
        message: `${t(locale, 'updateRevealFailed')}: ${serverMessage(err) || err?.message}`,
      })
    }
  }

  if (!enabled) return null

  const asset = info?.assets.find(item => item.current_platform)
  const done = progress?.state === 'done'
  // 下了 1.5.0 没装，之后再查到 1.6.0：文件确实在下载目录里，但不能再说「双击即可完成更新」。
  // 不整块藏掉（那是把真实存在的文件藏起来），改成说明它是要旧的那个。
  const stale = done && Boolean(info && progress?.version !== info.latest)
  // 下载失败要留在屏上：进度条随 state 离开 downloading 就消失了，
  // 一条会自己关掉的 toast 等于没说。后端记的原因优先，本地兜底次之。
  const downloadErrorNode =
    progress?.state === 'error'
      ? progress.error
        ? t(locale, 'updateDownloadError', progress.error)
        : t(locale, 'updateDownloadFailed')
      : startError
        ? t(locale, 'updateDownloadError', startError)
        : null
  const received = progress?.received ?? 0
  const total = progress?.total ?? 0
  const percent = total > 0 ? Math.min(100, Math.round((received / total) * 100)) : 0

  return (
    <div className="console-update">
      {/* 没查过就不显示这一行：一个破折号分不清「还没查」和「查不到」 */}
      {(info || error) && (
        <div className="console-db-line">
          <span>{t(locale, 'updateLatest')}</span>
          {error ? (
            <span className="console-update-err">{t(locale, 'updateQueryFailed')}</span>
          ) : (
            info && (
              <span className={info.has_update ? 'console-update-new' : undefined}>
                v{info.latest}
                {info.has_update && asset ? ` · ${formatSize(asset.size)}` : ''}
                {info.has_update && info.published_at
                  ? ` · ${new Date(info.published_at).toLocaleDateString()}`
                  : ''}
              </span>
            )
          )}
        </div>
      )}

      {info && !info.has_update && (
        <div className="console-note">{t(locale, 'updateUpToDate', info.current)}</div>
      )}
      {info?.has_update && !asset && (
        <div className="console-note warn">{t(locale, 'updateNoAsset')}</div>
      )}
      {error && (
        <div className="console-note warn">{t(locale, 'updateCheckFailed', error)}</div>
      )}

      {downloading && (
        <div className="console-update-progress">
          <Progress
            value={percent}
            size="sm"
            radius="sm"
            aria-label={t(locale, 'updateDownloading', progress?.name || '')}
          />
          <span className="console-update-progress-text">
            {total > 0
              ? `${formatSize(received)} / ${formatSize(total)} · ${percent}%`
              : formatSize(received)}
          </span>
        </div>
      )}

      {downloadErrorNode && (
        <div className="console-note warn" role="status">
          {downloadErrorNode}
        </div>
      )}

      {done && (
        <div className="console-update-done">
          <div className="console-update-done-path">{progress?.path || progress?.name}</div>
          <div className="console-update-hint">
            {stale
              ? t(locale, 'updateInstalledOlder', `v${progress?.version ?? ''}`)
              : t(locale, 'updateInstallHint')}
          </div>
        </div>
      )}

      <div className="console-route-actions">
        <Button
          size="xs"
          variant="default"
          leftSection={<IconRefresh size={13} />}
          loading={checking}
          onClick={() => void onCheck(true)}
        >
          {t(locale, 'updateCheck')}
        </Button>
        {info?.has_update && asset && !downloading && (
          <Button size="xs" leftSection={<IconDownload size={13} />} onClick={() => void onDownload()}>
            {t(locale, 'updateDownload', `v${info.latest}`)}
          </Button>
        )}
        {done && (
          <Button
            size="xs"
            variant="default"
            leftSection={<IconFolderOpen size={13} />}
            onClick={() => void onReveal()}
          >
            {t(locale, 'updateReveal')}
          </Button>
        )}
        {info?.release_url && (
          <Button
            size="xs"
            variant="default"
            leftSection={<IconExternalLink size={13} />}
            onClick={() => window.open(info.release_url, '_blank', 'noopener,noreferrer')}
          >
            {t(locale, 'updateReleasePage')}
          </Button>
        )}
      </div>
    </div>
  )
}
