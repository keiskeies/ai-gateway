import { useEffect } from 'react'
import Dashboard from './Dashboard'

export default function Wall() {
  useEffect(() => {
    document.documentElement.classList.add('wall-route')
    return () => document.documentElement.classList.remove('wall-route')
  }, [])

  return (
    <div className="wall-route">
      <Dashboard />
    </div>
  )
}
