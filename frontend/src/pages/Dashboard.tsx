import { useEffect, useState } from 'react'
import { Card, Col, Row, Statistic, Tag, Typography } from 'antd'
import {
  CloudServerOutlined, RobotOutlined, ApiOutlined,
  CheckCircleOutlined, CloseCircleOutlined, ThunderboltOutlined,
} from '@ant-design/icons'
import { getOverview, listProxies, listPlatforms } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { getPresetName, platformPresets } from '../presets'

export default function Dashboard() {
  const [stats, setStats] = useState<any>({})
  const [proxies, setProxies] = useState<any[]>([])
  const [platforms, setPlatforms] = useState<any[]>([])
  const { locale } = useAppContext()

  useEffect(() => { loadData() }, [])

  const loadData = async () => {
    try {
      const [overview, proxyList, platformList] = await Promise.all([
        getOverview().catch(() => ({})),
        listProxies().catch(() => []),
        listPlatforms().catch(() => []),
      ])
      setStats(overview)
      setProxies(proxyList)
      setPlatforms(platformList)
    } catch {}
  }

  const getPlatformDisplayName = (name: string) => {
    const preset = platformPresets.find(p => p.name === name)
    return preset ? getPresetName(preset, locale) : name
  }

  return (
    <div>
      <Row gutter={[16, 16]}>
        <Col xs={24} sm={12} lg={6}>
          <Card><Statistic title={t(locale, 'totalRequests')} value={stats.total_requests || 0} prefix={<ThunderboltOutlined />} /></Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card><Statistic title={t(locale, 'successRate')} value={stats.success_rate || 0} suffix="%" precision={1} valueStyle={{ color: (stats.success_rate || 0) > 90 ? '#3f8600' : '#cf1322' }} /></Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card><Statistic title={t(locale, 'avgLatency')} value={stats.avg_latency_ms || 0} suffix="ms" /></Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card><Statistic title={t(locale, 'activePlatforms')} value={stats.active_platforms || 0} suffix={`/ ${stats.total_platforms || 0}`} prefix={<CloudServerOutlined />} /></Card>
        </Col>
      </Row>

      <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
        <Col span={24}>
          <Card title={t(locale, 'proxyStatus')} extra={<Statistic title={t(locale, 'activeCount')} value={stats.active_proxies || 0} suffix={`/ ${stats.total_proxies || 0}`} valueStyle={{ fontSize: 14 }} />}>
            {proxies.length === 0 ? (
              <Typography.Text type="secondary">{t(locale, 'noProxies')}</Typography.Text>
            ) : (
              <Row gutter={[12, 12]}>
                {proxies.map((p: any) => (
                  <Col xs={24} sm={12} lg={8} key={p.id}>
                    <Card size="small" style={{ borderLeft: `3px solid ${p.status === 'Running' ? '#52c41a' : '#d9d9d9'}` }}>
                      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                        <div>
                          <div style={{ fontWeight: 600 }}>{p.name}</div>
                          <div style={{ fontSize: 12, color: '#999' }}>{t(locale, 'port')} {p.listen_port}</div>
                        </div>
                        <Tag color={p.status === 'Running' ? 'success' : 'default'} icon={p.status === 'Running' ? <CheckCircleOutlined /> : <CloseCircleOutlined />}>
                          {p.status === 'Running' ? t(locale, 'running') : t(locale, 'stopped')}
                        </Tag>
                      </div>
                      <div style={{ marginTop: 8 }}>
                        {(p.protocols || []).map((pt: string) => (
                          <Tag key={pt} color="blue" style={{ fontSize: 11 }}>{pt}</Tag>
                        ))}
                      </div>
                    </Card>
                  </Col>
                ))}
              </Row>
            )}
          </Card>
        </Col>
      </Row>

      <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
        <Col span={24}>
          <Card title={t(locale, 'platformList')}>
            {platforms.length === 0 ? (
              <Typography.Text type="secondary">{t(locale, 'noPlatforms')}</Typography.Text>
            ) : (
              <Row gutter={[12, 12]}>
                {platforms.map((p: any) => (
                  <Col xs={24} sm={12} lg={6} key={p.id}>
                    <Card size="small" hoverable>
                      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                        <strong>{getPlatformDisplayName(p.name)}</strong>
                        <Tag color={p.status === 'Active' ? 'success' : 'default'}>{p.type}</Tag>
                      </div>
                      <div style={{ fontSize: 12, color: '#999', marginTop: 4, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        {p.base_url}
                      </div>
                    </Card>
                  </Col>
                ))}
              </Row>
            )}
          </Card>
        </Col>
      </Row>
    </div>
  )
}
