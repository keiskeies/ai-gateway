import { useEffect } from 'react'
import { IconX } from '@tabler/icons-react'
import { useNavigate } from 'react-router-dom'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import Dashboard from './Dashboard'

export default function Wall() {
  const navigate = useNavigate()
  const { locale } = useAppContext()

  useEffect(() => {
    document.documentElement.classList.add('wall-route')
    return () => document.documentElement.classList.remove('wall-route')
  }, [])

  return (
    <div className="wall-route">
      <Dashboard />
      <button type="button" className="wall-route-exit" onClick={() => navigate('/')}>
        <IconX size={14} /> {t(locale, 'exitWall')}
      </button>
    </div>
  )
}
