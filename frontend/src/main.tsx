import ReactDOM from 'react-dom/client'
import {
  MantineProvider, createTheme, Button, ActionIcon, Modal, Table, TextInput,
  Select, Textarea, NumberInput, Badge, Card, Paper, Tabs, SegmentedControl,
  Menu, Tooltip, Switch, Checkbox, PasswordInput, Notification, Loader, Pagination, Alert,
} from '@mantine/core'
import { Notifications } from '@mantine/notifications'
import { HashRouter } from 'react-router-dom'
import { AppProvider, useAppContext } from './ThemeContext'
import App from './App'
import { initConfig } from './api'
import '@mantine/core/styles.css'
import '@mantine/notifications/styles.css'
import '@mantine/charts/styles.css'
import './main.css'

initConfig()

/* 内联防闪底色（index.html）会在 <html> 上写死 style.background，
   这会盖住 CSS 变量，导致切换主题时底色不跟随。挂载前清掉它。 */
document.documentElement.style.removeProperty('background')
document.body?.style.removeProperty('background')

/* ---- Deep Navy Analytics 主题 ----
   单一强调色 #5C9DFF；gray/dark 调色板整体染成蓝调，
   夜间模式下所有 Mantine 组件（含未覆盖的默认表面）不再透出中性灰。
   页面代码不散落视觉参数，统一由此处 + main.css token 控制。 */
const theme = createTheme({
  primaryColor: 'brand',
  primaryShade: { light: 6, dark: 4 },
  /* 填充按钮/徽标文字按底色亮度自动取深墨或纯白：
     夜间模式下 #5C9DFF 亮蓝底自动配深字，避免白字发虚 */
  autoContrast: true,
  colors: {
    brand: [
      '#E7F0FF', '#CFE1FF', '#A8C8FF', '#7FACFF', '#5C9DFF',
      '#3F7EE0', '#2E63B8', '#244E92', '#1B3B6E', '#122747',
    ],
    /* 亮灰阶：蓝灰 slate，供 gray-0..9 引用处保持冷色调 */
    gray: [
      '#EEF2FB', '#DCE3F1', '#BCC8DF', '#93A3C4', '#6E80A9',
      '#55658C', '#404E71', '#2E3A58', '#1F2942', '#121A2E',
    ],
    /* 暗色模式的中性面：深海军蓝三阶，替代 Mantine 默认的近黑灰 */
    dark: [
      '#F2F6FF', '#C9D6F2', '#93A8D1', '#5E77AD', '#3A5387',
      '#27406F', '#1A3057', '#121F3D', '#0A142E', '#060D24',
    ],
    red: [
      '#FFE3E6', '#FFC9CF', '#FF9FA9', '#FF7F8D', '#FF6B7A',
      '#F04356', '#D2263B', '#A61B2E', '#7D1524', '#520E19',
    ],
    green: [
      '#D8F7EA', '#AEEED6', '#7CE0BC', '#4FD3A3', '#35D399',
      '#22B083', '#198F69', '#146E54', '#0F513F', '#0A3529',
    ],
    yellow: [
      '#FEF4DA', '#FBE5AC', '#F7D37C', '#F3C254', '#EEB135',
      '#D99A1F', '#B57D17', '#8C6113', '#63460F', '#3E2C09',
    ],
  },
  defaultRadius: 'md',
  cursorType: 'pointer',
  fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif",
  fontFamilyMonospace: "ui-monospace, 'Cascadia Mono', Consolas, monospace",
  headings: { fontWeight: '600' },
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

function ThemedApp() {
  const { isDark } = useAppContext()
  return (
    <MantineProvider theme={theme} forceColorScheme={isDark ? 'dark' : 'light'}>
      <Notifications position="top-right" />
      <HashRouter>
        <App />
      </HashRouter>
    </MantineProvider>
  )
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <AppProvider>
    <ThemedApp />
  </AppProvider>,
)
