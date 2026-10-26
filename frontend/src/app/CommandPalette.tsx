import { useState } from 'react'
import { Modal, TextInput } from '@mantine/core'
import { IconSearch } from '@tabler/icons-react'
import { useNavigate } from 'react-router-dom'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { NAV_GROUPS } from './nav'

export function CommandPalette({ opened, onClose, localConsole }: { opened: boolean; onClose: () => void; localConsole: boolean }) {
  const navigate = useNavigate()
  const { locale } = useAppContext()
  const [query, setQuery] = useState('')

  const groups = localConsole
    ? NAV_GROUPS
    : NAV_GROUPS
        .map(group => ({ ...group, items: group.items.filter(item => item.path !== '/clients') }))
        .filter(group => group.items.length > 0)
  const items = groups.flatMap(group => group.items)
  const filtered = items.filter(item => {
    const label = t(locale, item.label).toLowerCase()
    const group = t(locale, groups.find(g => g.items.includes(item))?.label || 'navRun').toLowerCase()
    return `${label} ${group} ${item.path}`.includes(query.trim().toLowerCase())
  })

  const close = () => {
    setQuery('')
    onClose()
  }

  const go = (path: string) => {
    navigate(path)
    close()
  }

  return (
    <Modal
      opened={opened}
      onClose={close}
      title={t(locale, 'commandPalette')}
      size="md"
      closeOnClickOutside
      transitionProps={{ transition: 'fade', duration: 140 }}
    >
      <TextInput
        autoFocus
        leftSection={<IconSearch size={15} />}
        placeholder={t(locale, 'commandPlaceholder')}
        value={query}
        onChange={event => setQuery(event.currentTarget.value)}
        mb="sm"
      />
      <div className="console-palette-list">
        {filtered.map(item => {
          const Icon = item.icon
          return (
            <button
              key={item.path}
              type="button"
              className="console-palette-item"
              onClick={() => go(item.path)}
            >
              <Icon size={15} stroke={1.7} />
              <span>{t(locale, item.label)}</span>
              <kbd>{item.path}</kbd>
            </button>
          )
        })}
        {filtered.length === 0 && <div className="console-palette-empty">{t(locale, 'noData')}</div>}
      </div>
    </Modal>
  )
}
