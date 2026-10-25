import ReactDOM from 'react-dom/client'
import {
  MantineProvider, createTheme, defaultVariantColorsResolver, Button, ActionIcon, Modal, Table, TextInput,
  Select, Textarea, NumberInput, Badge, Card, Paper, Tabs, SegmentedControl,
  Menu, Tooltip, Switch, Checkbox, PasswordInput, Notification, Loader, Pagination, Alert,
} from '@mantine/core'
import { Notifications } from '@mantine/notifications'
import { QueryClientProvider } from '@tanstack/react-query'
import { HashRouter } from 'react-router-dom'
import { AppProvider, useAppContext } from './ThemeContext'
import App from './App'
import { AppStateProvider } from './app/AppState'
import { queryClient } from './app/queryClient'
import { initConfig } from './api'
import '@fontsource/archivo/latin-500.css'
import '@fontsource/archivo/latin-600.css'
import '@fontsource/archivo/latin-700.css'
import '@fontsource/ibm-plex-sans/latin-400.css'
import '@fontsource/ibm-plex-sans/latin-500.css'
import '@fontsource/ibm-plex-sans/latin-600.css'
import '@fontsource/ibm-plex-mono/latin-400.css'
import '@fontsource/ibm-plex-mono/latin-500.css'
import '@mantine/core/styles.css'
import '@mantine/notifications/styles.css'
import '@mantine/charts/styles.css'
import './main.css'
import './app/console.css'

initConfig()

/* 内联防闪底色（index.html）会在 <html> 上写死 style.background，
   这会盖住 CSS 变量，导致切换主题时底色不跟随。挂载前清掉它。 */
document.documentElement.style.removeProperty('background')
document.body?.style.removeProperty('background')

/* ---- 双主题：暗色 = 石墨 + 亮青/金，亮色 = 白纸 + 深青/金 ----
   两套都从 logo 的青 + 金取色；暗色用中性石墨承托，亮色用中性白纸承托。
   页面代码不散落视觉参数，统一由此处 + main.css token 控制。 */

/* 填充面的文字色交给 CSS token（--accent-ink）：
   暗色青底配深墨、浅色蓝底配白字；避免 autoContrast 用错方案的色阶。 */
const variantColorResolver = (input: Parameters<typeof defaultVariantColorsResolver>[0]) => {
  const colors = defaultVariantColorsResolver(input)
  const brand = !input.color || input.color === 'brand'
  if (input.variant === 'filled' && brand) return { ...colors, color: 'var(--accent-ink)' }
  return colors
}

const lightTheme = createTheme({
  primaryColor: 'brand',
  primaryShade: { light: 6, dark: 4 },
  /* 填充按钮/徽标文字按底色亮度自动取深墨或纯白；
     brand 填充面再由 variantColorResolver 统一到 --accent-ink */
  autoContrast: true,
  variantColorResolver,
  colors: {
    brand: [
      '#E6F6F4', '#C4EDE9', '#9ADFD9', '#6FCFC8', '#45C8C3',
      '#2A9D97', '#0F6E6A', '#0B5754', '#08413F', '#052B2A',
    ],
    /* 亮色方案：中性灰；暗色方案在 darkColors 中覆盖为石墨 */
    gray: [
      '#F7F8F6', '#EDEFEB', '#DDE1DB', '#C3C9C2', '#9AA39B',
      '#747D75', '#59625A', '#404842', '#2A302C', '#171C18',
    ],
    /* 浅色主题不渲染暗色面；保留一套石墨暗面，暗色主题在 darkColors 中覆盖 */
    dark: [
      '#F1F3F1', '#B8BEBA', '#858D88', '#5F6661', '#474E4A',
      '#2E3431', '#1E2323', '#111414', '#0B0D0D', '#070909',
    ],
    red: [
      '#FFE3E6', '#FFC9CF', '#FF9FA9', '#FF7F8D', '#FF6B7A',
      '#F04356', '#D2263B', '#A61B2E', '#7D1524', '#520E19',
    ],
    green: [
      '#E9F7EC', '#CDEED4', '#A6DEB2', '#7CCB8D', '#55B96C',
      '#3A9E54', '#2B7F44', '#216636', '#174A28', '#0E2F1A',
    ],
    yellow: [
      '#FEF4E2', '#FBE6C4', '#F6D49B', '#F0C06F', '#E9AB45',
      '#CE8F2A', '#A8751D', '#8A5A00', '#633F00', '#402900',
    ],
  },
  defaultRadius: 'md',
  cursorType: 'pointer',
  fontFamily: "'IBM Plex Sans', -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Microsoft YaHei', sans-serif",
  fontFamilyMonospace: "'IBM Plex Mono', ui-monospace, 'Cascadia Mono', Consolas, monospace",
  headings: { fontFamily: "'Archivo', 'IBM Plex Sans', -apple-system, sans-serif", fontWeight: '600' },
  fontSizes: { xs: '11px', sm: '12px', md: '13px', lg: '15px', xl: '18px' },
  components: {
    Button: Button.extend({ defaultProps: { size: 'sm', radius: 'md' } }),
    ActionIcon: ActionIcon.extend({ defaultProps: { variant: 'subtle', radius: 'md' } }),
    Modal: Modal.extend({
      defaultProps: {
        centered: true,
        radius: 'lg',
        overlayProps: { backgroundOpacity: 0.62, blur: 3 },
        transitionProps: { transition: 'fade', duration: 180 },
      },
    }),
    Table: Table.extend({
      defaultProps: { verticalSpacing: 'sm', horizontalSpacing: 'md', highlightOnHover: true, striped: false },
    }),
    TextInput: TextInput.extend({ defaultProps: { size: 'sm' } }),
    PasswordInput: PasswordInput.extend({ defaultProps: { size: 'sm' } }),
    NumberInput: NumberInput.extend({ defaultProps: { size: 'sm' } }),
    Textarea: Textarea.extend({ defaultProps: { size: 'sm', autosize: true, minRows: 3 } }),
    Select: Select.extend({ defaultProps: { size: 'sm', comboboxProps: { shadow: 'none' } } }),
    Badge: Badge.extend({ defaultProps: { radius: 'sm', variant: 'light', size: 'md' } }),
    Card: Card.extend({ defaultProps: { radius: 'md', withBorder: true, shadow: 'none' } }),
    Paper: Paper.extend({ defaultProps: { radius: 'md', shadow: 'none' } }),
    Tabs: Tabs.extend({ defaultProps: { variant: 'pills' } }),
    SegmentedControl: SegmentedControl.extend({ defaultProps: { size: 'xs' } }),
    Menu: Menu.extend({ defaultProps: { shadow: 'none', radius: 'md' } }),
    Tooltip: Tooltip.extend({ defaultProps: { withArrow: true, openDelay: 200, radius: 'md' } }),
    Switch: Switch.extend({ defaultProps: { size: 'sm' } }),
    Checkbox: Checkbox.extend({ defaultProps: { size: 'sm' } }),
    Notification: Notification.extend({ defaultProps: { radius: 'md', withBorder: true } }),
    Loader: Loader.extend({ defaultProps: { size: 'sm', type: 'oval' } }),
    Pagination: Pagination.extend({ defaultProps: { size: 'sm', radius: 'md' } }),
    Alert: Alert.extend({ defaultProps: { radius: 'md', variant: 'light' } }),
  },
})

/* 暗色方案：中性石墨表面 + 中性文字 + 亮青/金。
   只覆盖颜色，字体、圆角、组件默认值全部沿用 lightTheme。 */
const darkColors: typeof lightTheme.colors = {
  ...lightTheme.colors,
  brand: [
    '#E3F8F6', '#C0F0EC', '#94E6DF', '#6BD9D1', '#45C8C3',
    '#31A9A5', '#268A87', '#1D6C6A', '#15504F', '#0E3635',
  ],
  gray: [
    '#F7F8F7', '#E4E7E5', '#CDD2CF', '#B0B7B2', '#8E9691',
    '#6E7671', '#515853', '#3A413D', '#262C28', '#171B1B',
  ],
  dark: [
    '#F1F3F1', '#B8BEBA', '#858D88', '#5F6661', '#474E4A',
    '#2E3431', '#1E2323', '#111414', '#0B0D0D', '#070909',
  ],
  red: [
    '#FDE7EA', '#F9C9D0', '#F5A6B1', '#F28C99', '#F07C86',
    '#D45E6C', '#B24653', '#8E343F', '#6A252D', '#46161C',
  ],
  green: [
    '#E7F8EF', '#C9EFDC', '#A4E3C4', '#82D9AE', '#6FD39A',
    '#56B87F', '#3F9967', '#2F7A52', '#225B3E', '#153D2A',
  ],
  yellow: [
    '#FCF3DC', '#F6E4B7', '#F0D38E', '#ECC469', '#E8B75C',
    '#C99A42', '#A67D2E', '#826120', '#5E4516', '#3D2C0D',
  ],
}
const darkTheme = { ...lightTheme, colors: darkColors }

function ThemedApp() {
  const { isDark } = useAppContext()
  return (
    <MantineProvider theme={isDark ? darkTheme : lightTheme} forceColorScheme={isDark ? 'dark' : 'light'}>
      <Notifications position="top-right" />
      <QueryClientProvider client={queryClient}>
        <AppStateProvider>
          <HashRouter>
            <App />
          </HashRouter>
        </AppStateProvider>
      </QueryClientProvider>
    </MantineProvider>
  )
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <AppProvider>
    <ThemedApp />
  </AppProvider>,
)
