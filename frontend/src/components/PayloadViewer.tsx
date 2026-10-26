import { useMemo, useState } from 'react'
import { t, type Locale, type TranslationKey } from '../i18n'
import { parseRequestPayload, parseResponsePayload, prettyRawBody } from '../lib/payloadView'
import type { PayloadBlock, PayloadKeyValue, PayloadMessage } from '../lib/payloadView'
import type { RequestPayload, RequestPayloadResponse } from '../types/stats'

const ROLE_KEYS: Record<string, TranslationKey> = {
  system: 'roleSystem',
  user: 'roleUser',
  assistant: 'roleAssistant',
  tool: 'roleTool',
}

const encoder = new TextEncoder()

function bytesLabel(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${bytes} B`
}

function storedBytes(body: string): number {
  return encoder.encode(body).length
}

function roleLabel(role: string, locale: Locale): string {
  const key = ROLE_KEYS[role]
  return key ? t(locale, key) : role
}

function CaptureMissing({ locale, enabled }: { locale: Locale; enabled: boolean }) {
  return (
    <div className="console-note">
      {enabled ? t(locale, 'payloadNotRecorded') : t(locale, 'payloadCaptureOff')}
      {!enabled && <> {t(locale, 'payloadCaptureHint')}</>}
    </div>
  )
}

function TruncatedNote({ locale, stored, original }: { locale: Locale; stored: number; original: number }) {
  return (
    <div className="console-note warn">
      {original > stored
        ? t(locale, 'payloadTruncatedPrefix', bytesLabel(stored), bytesLabel(original))
        : t(locale, 'payloadTruncated')}
    </div>
  )
}

function RawDetails({ label, body }: { label: string; body: string }) {
  // 1 MB 的原始 SSE 只应在用户真的展开时才进 DOM；details 里的子节点默认也会渲染
  const [open, setOpen] = useState(false)
  return (
    <details className="console-payload-raw" onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary>{label}</summary>
      {open && <pre className="console-payload-body">{prettyRawBody(body)}</pre>}
    </details>
  )
}

function KeyValueList({ entries }: { entries: PayloadKeyValue[] }) {
  return (
    <div className="console-payload-kv">
      {entries.map((entry) => (
        <div key={entry.name} style={{ display: 'contents' }}>
          <span className="k">{entry.name}</span>
          <span className="v">{entry.value}</span>
        </div>
      ))}
    </div>
  )
}

function ToolCallBlock({ block, locale }: { block: Extract<PayloadBlock, { kind: 'tool_use' }>; locale: Locale }) {
  return (
    <div className="console-payload-tool">
      <div className="console-payload-tool-head">
        <span>{t(locale, 'payloadToolCall')}</span>
        <span className="console-mono" style={{ color: 'var(--text-1)' }}>{block.name}</span>
        {block.id && <span className="console-payload-chip">{block.id}</span>}
      </div>
      <pre className="console-payload-code">{block.input || '{}'}</pre>
    </div>
  )
}

function BlockView({ block, locale }: { block: PayloadBlock; locale: Locale }) {
  switch (block.kind) {
    case 'text':
      return <div className="console-payload-text">{block.text}</div>
    case 'thinking':
      return (
        <details className="console-payload-thinking">
          <summary>{t(locale, 'payloadThinking')}</summary>
          <div className="console-payload-text">{block.text}</div>
        </details>
      )
    case 'tool_use':
      return <ToolCallBlock block={block} locale={locale} />
    case 'tool_result':
      return (
        <div className="console-payload-tool">
          <div className="console-payload-tool-head">
            <span>{t(locale, 'payloadToolResult')}</span>
            {block.toolUseId && <span className="console-payload-chip">{block.toolUseId}</span>}
          </div>
          <div className="console-payload-text">{block.text}</div>
        </div>
      )
    case 'image':
      return <span className="console-payload-chip">{t(locale, 'payloadImage')} · {block.label}</span>
    case 'unknown':
      return (
        <details className="console-payload-thinking">
          <summary>{block.label}</summary>
          <pre className="console-payload-code">{block.json}</pre>
        </details>
      )
  }
}

function PayloadBlocks({ blocks, locale }: { blocks: PayloadBlock[]; locale: Locale }) {
  if (!blocks.length) {
    return <div className="console-payload-empty">{t(locale, 'payloadNoContent')}</div>
  }
  return (
    <div className="console-payload-blocks">
      {blocks.map((block, index) => <BlockView key={index} block={block} locale={locale} />)}
    </div>
  )
}

function MessageView({ message, locale }: { message: PayloadMessage; locale: Locale }) {
  return (
    <div className={`console-payload-msg role-${message.role}`}>
      <div className="console-payload-msg-head">
        <span className="console-payload-role">{roleLabel(message.role, locale)}</span>
        {message.name && <span className="console-payload-chip">{message.name}</span>}
        {message.toolCallId && <span className="console-payload-chip">tool_call_id: {message.toolCallId}</span>}
      </div>
      <PayloadBlocks blocks={message.blocks} locale={locale} />
    </div>
  )
}

function MetaRow({ items }: { items: { label: string; value: string }[] }) {
  return (
    <div className="console-payload-meta">
      {items.map((item, index) => (
        <span key={`${item.label}-${index}`}>
          {item.label && <><span className="k">{item.label}</span>{' '}</>}{item.value}
        </span>
      ))}
    </div>
  )
}

export function RequestPayloadView({ payload, locale }: { payload: RequestPayload; locale: Locale }) {
  const { request } = payload
  const parsed = useMemo(
    () => (request.body ? parseRequestPayload(request.body, request.body_truncated, request.protocol) : null),
    [request.body, request.body_truncated, request.protocol],
  )

  if (!request.body_captured) {
    return <CaptureMissing locale={locale} enabled={payload.request_capture_enabled} />
  }
  if (!request.body) {
    return <div className="console-payload-empty">{t(locale, 'payloadEmptyBody')}</div>
  }
  const stored = storedBytes(request.body)
  return (
    <div className="console-payload-view">
      {request.body_truncated && <TruncatedNote locale={locale} stored={stored} original={request.bytes} />}
      {!parsed || parsed.parseError === 'empty' ? (
        <div className="console-payload-empty">{t(locale, 'payloadEmptyBody')}</div>
      ) : parsed.parseError ? (
        <>
          <div className="console-note warn">{t(locale, 'payloadParseFailed')}</div>
          <pre className="console-payload-body">{prettyRawBody(parsed.raw)}</pre>
        </>
      ) : (
        <>
          <MetaRow
            items={[
              ...(parsed.model ? [{ label: 'model', value: parsed.model }] : []),
              ...(parsed.stream ? [{ label: '', value: t(locale, 'payloadStream') }] : []),
              { label: t(locale, 'payloadMessages'), value: String(parsed.messages.length) },
              ...(parsed.tools.length ? [{ label: t(locale, 'payloadTools'), value: String(parsed.tools.length) }] : []),
            ]}
          />
          {parsed.system && (
            <div className="console-payload-section">
              <div className="console-payload-section-title">{t(locale, 'payloadSystem')}</div>
              <div className="console-payload-msg role-system">
                <div className="console-payload-text">{parsed.system}</div>
              </div>
            </div>
          )}
          {parsed.messages.length > 0 && (
            <div className="console-payload-section">
              <div className="console-payload-section-title">{t(locale, 'payloadMessages')}</div>
              {parsed.messages.map((message, index) => (
                <MessageView key={index} message={message} locale={locale} />
              ))}
            </div>
          )}
          {parsed.params.length > 0 && (
            <div className="console-payload-section">
              <div className="console-payload-section-title">{t(locale, 'payloadParameters')}</div>
              <KeyValueList entries={parsed.params} />
            </div>
          )}
          {parsed.tools.length > 0 && (
            <div className="console-payload-section">
              <div className="console-payload-section-title">{t(locale, 'payloadTools')}</div>
              {parsed.tools.map((tool) => (
                <div key={tool.name} className="console-payload-tool-def">
                  <div className="console-mono" style={{ color: 'var(--text-1)', fontSize: 12 }}>{tool.name}</div>
                  {tool.description && <div className="console-payload-desc">{tool.description}</div>}
                  {tool.schema && (
                    <details className="console-payload-raw">
                      <summary>{t(locale, 'payloadToolSchema')}</summary>
                      <pre className="console-payload-code">{tool.schema}</pre>
                    </details>
                  )}
                </div>
              ))}
            </div>
          )}
          <RawDetails label={t(locale, 'payloadRawJson')} body={parsed.raw} />
        </>
      )}
    </div>
  )
}

export function ResponsePayloadView({
  response,
  captureEnabled,
  locale,
}: {
  response: RequestPayloadResponse
  captureEnabled: boolean
  locale: Locale
}) {
  const parsed = useMemo(
    () => (response.body
      ? parseResponsePayload(response.body, response.content_type, response.truncated, response.protocol)
      : null),
    [response.body, response.content_type, response.truncated, response.protocol],
  )

  if (!response.captured) {
    return <CaptureMissing locale={locale} enabled={captureEnabled} />
  }
  if (!response.body) {
    return <div className="console-payload-empty">{t(locale, 'payloadEmptyBody')}</div>
  }
  const stored = storedBytes(response.body)
  return (
    <div className="console-payload-view">
      <div className="console-note">
        {t(locale, 'payloadRawUpstream')} · {response.protocol ?? '—'} · {response.status_code ?? '—'} · {response.content_type ?? '—'}
      </div>
      {response.truncated && <TruncatedNote locale={locale} stored={stored} original={response.bytes} />}
      {!parsed || parsed.parseError === 'empty' ? (
        <div className="console-payload-empty">{t(locale, 'payloadEmptyBody')}</div>
      ) : parsed.parseError ? (
        <>
          <div className="console-note warn">{t(locale, 'payloadParseFailed')}</div>
          <pre className="console-payload-body">{prettyRawBody(parsed.raw)}</pre>
        </>
      ) : (
        <>
          <MetaRow
            items={[
              ...(parsed.model ? [{ label: 'model', value: parsed.model }] : []),
              ...(parsed.stream ? [{ label: '', value: t(locale, 'payloadStream') }] : []),
              ...(parsed.events != null ? [{ label: t(locale, 'payloadEvents'), value: String(parsed.events) }] : []),
              ...(parsed.finishReason ? [{ label: t(locale, 'payloadFinishReason'), value: parsed.finishReason }] : []),
            ]}
          />
          <div className="console-payload-section">
            <PayloadBlocks blocks={parsed.blocks} locale={locale} />
          </div>
          {parsed.usage.length > 0 && (
            <div className="console-payload-section">
              <div className="console-payload-section-title">{t(locale, 'payloadUsage')}</div>
              <KeyValueList entries={parsed.usage} />
            </div>
          )}
          <RawDetails
            label={t(locale, parsed.stream ? 'payloadRawSse' : 'payloadRawJson')}
            body={parsed.raw}
          />
        </>
      )}
    </div>
  )
}
