<p align="center">
  <img src="logo.png" width="100" alt="AI Gateway Logo" />
</p>

<h1 align="center">🚀 AI Gateway</h1>

<p align="center">
  <strong>Ein Gateway für alle KI-Plattformen</strong><br/>
  Lastverteilung über mehrere Schlüssel · Wiederholungen und Backend-Wechsel · OpenAI- &amp; Anthropic-Protokoll · keine Änderungen am Client
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2021-orange?logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/TypeScript-6.x-blue?logo=typescript" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-green" alt="Platform" />
  <img src="https://img.shields.io/badge/License-MIT-yellow" alt="License" />
  <a href="https://github.com/keiskeies/ai-gateway/stargazers"><img src="https://img.shields.io/github/stars/keiskeies/ai-gateway?style=social" alt="Stars" /></a>
</p>

<p align="center">
  <a href="README.md">中文</a> ·
  <a href="README_EN.md">English</a> ·
  <strong>Deutsch</strong> ·
  <a href="README_FR.md">Français</a> ·
  <a href="README_ES.md">Español</a> ·
  <a href="README_PT.md">Português</a>
</p>

<p align="center">
  <a href="#-schnellstart"><strong>Schnellstart →</strong></a>
</p>

---

> 🌟 Wenn dieses Projekt dir hilft, hilft ein Star anderen, es zu finden.

---

## 😩 Welches Problem wird gelöst

- Du hast mehrere Schlüssel für denselben Anbieter und möchtest sie gemeinsam nutzen, ohne eigene Rotation und Retry-Logik zu schreiben
- Du möchtest automatische Wiederholungen bei 429 / 5xx / Upstream-Timeouts, statt sie in jedem Client neu zu implementieren
- OpenAI und Anthropic bedeuten zwei SDKs, zwei Base-URLs und zwei Nachrichtenformate
- Kostenlose Kontingente und bezahlte Modelle werden gemischt, während Kosten, Kontingente und Fehlerursachen an verschiedenen Stellen liegen
- Du möchtest Schlüssel an ein Team verteilen, ohne die Zugangsdaten der Anbieter herauszugeben

AI Gateway bündelt all das in einem lokalen Prozess: **ein Einstiegspunkt, ein Satz Schlüssel, eine Statistik**.

---

## 📸 Oberfläche

Alle Screenshots stammen aus der aktuellen Desktop-App (dunkles Theme).

<p align="center">
  <img src="doc/console-wall.png" width="92%" alt="Monitoring-Dashboard" />
</p>
<p align="center"><em>Monitoring — KPI-Leiste plus Trends je Modell über acht Metriken (Anfragen / Fehler / Eingabe / Ausgabe / Latenz / TTFT / Geschwindigkeit / Cache) in Stunden-, Tages-, Monats- und Jahressicht, eine Aktivitäts-Heatmap (7d / 30d / 12m / Alle), Plattformanteile, eine Performance-Matrix und das Request-Log. Vollbild und Pausieren der Aktualisierung werden unterstützt</em></p>

<details>
<summary>📱 Weitere Seiten anzeigen</summary>

<p align="center">
  <img src="doc/console-traffic.png" width="92%" alt="Traffic" />
</p>
<p align="center"><em>Traffic — eine Übersicht „Aufmerksamkeit nötig" für Rate-Limits, Fehler und untätige Schlüssel; Anfragen lassen sich nach Modell, Plattform, Schlüssel, Status, Fehlertyp und Latenzbereich filtern und seitenweise durchsuchen; die Detailansicht zeigt die Versuchskette (Status und Dauer jedes Backend-Versuchs), TTFT-/Generierungsbalken, Cache-Lesen gegenüber neuer Eingabe und geschätzte Kosten und erlaubt das Kopieren der ID oder das erneute Senden der Anfrage</em></p>

<p align="center">
  <img src="doc/console-models.png" width="92%" alt="Virtuelle Modelle" />
</p>
<p align="center"><em>Virtuelle Modelle — Status, Anzahl der Backends, Strategie, Anfragevolumen, Erfolgsquote, durchschnittliche Latenz und Kosten direkt in der Liste</em></p>

<p align="center">
  <img src="doc/console-model-routing.png" width="92%" alt="Modell-Routing" />
</p>
<p align="center"><em>Modelldetail · Routing — links Strategie, Wiederholungen, Backoff, Standard-max_tokens und Gewichtsverteilung bearbeiten; in der Mitte zeichnet die Routing-Spine die Backends als gewichteten, nach Priorität geordneten Graphen; rechts lassen sich Anbieter, Fähigkeits-Badges und die Behandlung von Reasoning-Parametern eines einzelnen Backends prüfen</em></p>

<p align="center">
  <img src="doc/console-platforms.png" width="92%" alt="Plattformen" />
</p>
<p align="center"><em>Plattformen — Ergebnis der Konnektivitätsprüfung, Anzahl synchronisierter Modelle, Anzahl referenzierender virtueller Modelle, Anfragevolumen und Erfolgsquote; beeinträchtigte Plattformen werden im Kopfbereich gezählt</em></p>

<p align="center">
  <img src="doc/console-platform-detail.png" width="92%" alt="Plattformdetail" />
</p>
<p align="center"><em>Plattformdetail — Zugangsdaten und weitergeleitete Header (gespeicherte Schlüssel werden nur als Maske zurückgegeben, nie im Klartext), Abruf entfernter Modelle über „Verbindung testen", selektiver Modellimport von der Plattform, Preis-CSV-Import/-Export sowie die Frage, welche virtuellen Modelle von dieser Plattform abhängen</em></p>

<p align="center">
  <img src="doc/console-keys.png" width="92%" alt="API-Schlüssel" />
</p>
<p align="center"><em>API-Schlüssel — Anfragen, Tokens, durchschnittliche Latenz, Kosten und letzte Nutzung pro Schlüssel; ein Schlüssel kann an ein einzelnes virtuelles Modell gebunden werden</em></p>

<p align="center">
  <img src="doc/console-key-quota.png" width="92%" alt="Schlüssel-Kontingent" />
</p>
<p align="center"><em>Schlüssel-Kontingent — Ablaufdatum, Anfrage-Limit, Token-Limit, Kostenbudget und Kontingentfenster (lebenslang / Tag / Woche / Monat). Wird ein Limit erreicht, antwortet das Gateway mit 429; das Kostenbudget gilt nur für bepreiste Modelle</em></p>

<p align="center">
  <img src="doc/console-playground.png" width="92%" alt="Playground" />
</p>
<p align="center"><em>Playground — ein virtuelles Modell und ein Protokoll (OpenAI / Anthropic) auswählen und eine echte Anfrage senden, um Routing und Upstream zu prüfen; vor Ort einen temporären Schlüssel erzeugen oder das passende cURL kopieren</em></p>

<p align="center">
  <img src="doc/console-clients.png" width="92%" alt="Client-Zugriff" />
</p>
<p align="center"><em>Client-Zugriff — kartenbasierte Übernahme der lokalen Konfigurationen von Claude Code / Codex / OpenCode / OpenClaw / Pi / Hermes; beim Aufklappen erscheint der Diff, vor jedem Anwenden wird gesichert, Trennen rollt zurück</em></p>

<p align="center">
  <img src="doc/console-settings.png" width="92%" alt="Einstellungen" />
</p>
<p align="center"><em>Einstellungen — Port und Bind-Adresse, Standardwerte der Anfragepolitik, Kontosicherheit, Datenstatistik (Zeilen / Versuchsspuren / Alarme / verwaiste Zeilen / Aufbewahrung von Anfrageinhalten), Konfigurationsexport und -import sowie vollständige Datenbanksicherung und -wiederherstellung</em></p>

</details>

---

## ⚡ Kernfunktionen

### 🎯 Mehrere Schlüssel: mehrere Schlüssel eines Anbieters bündeln

Eine Plattform speichert genau einen Upstream-Zugangsschlüssel. Ein virtuelles Modell hat ein oder mehrere Backends. Um mehrere Schlüssel zu bündeln:

```
Eine Plattform pro Schlüssel anlegen → alle an dasselbe virtuelle Modell hängen → Round Robin oder Least Connections wählen
```

```
Deine Anwendung
   │
   └──→ Virtuelles Modell ──┬── Plattform A (Schlüssel 1, 3 RPM)
                            ├── Plattform B (Schlüssel 2, 3 RPM)
                            └── Plattform C (Schlüssel 3, 3 RPM)
```

Der Gesamtdurchsatz ist die Summe der einzelnen Schlüssellimits; das Gateway umgeht die Rate-Limits des Upstreams nicht.

### 🔀 Lastverteilungsstrategien

| Strategie | Verhalten | Geeignet für |
|-----------|-----------|--------------|
| **Round Robin** | Verteilt Anfragen der Reihe nach auf die Backends des virtuellen Modells | Schlüssel / Konten kombinieren |
| **Weighted Random** | Wählt ein Backend nach `weight` | Ungleiche Backend-Kapazität |
| **Least Connections** | Wählt das Backend mit den wenigsten Anfragen, die auf Upstream-Antwortheader warten | Spitzen bei kurzen Anfragen abfedern |
| **Priorität** | Wählt immer das aktive Backend mit dem kleinsten `priority`-Wert | Ein festes Primär-Backend |

> „Latency based" ist in der UI wählbar, verwendet derzeit aber die Round-Robin-Implementierung; die Latenz beeinflusst die Auswahl noch nicht. „Priorität" fällt bei einem Fehler des Primär-Backends auch nicht automatisch zurück. Siehe [Bekannte Einschränkungen](#-bekannte-einschränkungen).

### 🌐 OpenAI- und Anthropic-Protokolle

```
OpenAI SDK / OpenAI-kompatible Tools ──→ POST /v1/chat/completions ──┐
                                                                      ├──→ Routing → Upstream
Anthropic SDK ─────────────────────────→ POST /v1/messages ──────────┘
```

| Pfad | Protokoll | Hinweis |
|---|---|---|
| `/v1/chat/completions` | OpenAI | Haupteinstiegspunkt |
| `/v1/completions` | OpenAI | Alte Route, wird als Chat geparst |
| `/v1/messages` | Anthropic | Natives Anthropic-Format |
| `/v1/models` | OpenAI | Öffentlich; liefert virtuelle Modelle und Fähigkeitsfelder |

Nicht-streamende Antworten im selben Protokoll werden unverändert durchgereicht (nur `model` wird auf den Namen des virtuellen Modells zurückgesetzt); protokollübergreifende Antworten werden aus dem einheitlichen Format neu aufgebaut, Streaming-Antworten blockweise konvertiert.

### 🧠 Reasoning: der Client entscheidet

Welche Reasoning-Parameter der Client auch sendet, das Gateway übersetzt sie in das Format, das der Upstream versteht. Sendet der Client keine, schaltet das Gateway Reasoning nicht selbst ein.

| Client sendet | Upstream ist Claude | Upstream ist ein OpenAI-kompatibles Reasoning-Modell |
|---|---|---|
| `reasoning_effort: "high"` (OpenAI) | `thinking: {type: "enabled", budget_tokens: 16384}` | `reasoning_effort: "high"` |
| `thinking: {budget_tokens: 10000}` (Anthropic) | Bleibt erhalten; das Budget wird gekürzt, wenn `max_tokens` zu klein ist | Wird auf den entsprechenden Effort abgebildet |

- Stil pro Backend: `Auto` / OpenAI effort / Anthropic thinking / OpenRouter / Qwen / GLM / vLLM Qwen / aus
- `Auto` löst nach Upstream-Typ auf: `thinking` für Anthropic, sonst `reasoning_effort`; `param_overrides` wird zuletzt zusammengeführt und gewinnt
- Anthropic-Thinking-Signaturen werden unverändert wiedergegeben; protokollübergreifende Tool-Schleifen nutzen einen In-Memory-Signatur-Cache (30 Minuten, nach `tool_use.id`). Bei einem Cache-Miss wird Reasoning deaktiviert, statt eine Signatur zu erfinden
- `/v1/models` liefert für Reasoning-Modelle `capabilities` / `supported_parameters` / `reasoning`; ob ein Client eine Reasoning-Steuerung anzeigt, hängt weiterhin von seiner eigenen Modellfähigkeitstabelle ab

### 🛡️ Wiederholungen und Backend-Wechsel

- Retries pro Route: `max_retries` und `backoff_ms` sind in der UI einstellbar; wiederholt werden fest 429 / 5xx / 408
- Standardmäßig werden **429 / 5xx / 408 (Upstream-Timeout-Antworten)** mit exponentiellem Backoff wiederholt (500 ms × 2ⁿ)
- Jeder Versuch läuft erneut durch den Load Balancer; bei Round Robin landet er meist auf dem nächsten Backend
- Streaming-Anfragen nutzen ein separates, längeres `stream_timeout_secs` (Standard 1800 s), damit lange Generierungen nicht durch das normale Anfrage-Timeout abgeschnitten werden

> Verbindungsfehler (nicht erreichbares Backend, Dial-Fehler, Netzwerk-Timeout) werden standardmäßig nicht wiederholt, und es gibt keine gesundheitsprüfungsbasierte Auslistung. Wenn du „bei Ausfall eines Backends automatisch wechseln" brauchst, lege die Backends in ein virtuelles Modell hinter Round Robin / Least Connections oder deaktiviere das fehlerhafte Backend manuell. Siehe [Bekannte Einschränkungen](#-bekannte-einschränkungen).

### 📊 Monitoring-Dashboard

Ein vollständiges Leistungsprofil für jedes Modell in Stunden-, Tages-, Monats- und Jahressicht:

- **Acht Metrik-Trends**: Anfragen / Fehler / Eingabe / Ausgabe / Latenz / Time-to-first-token (TTFT) / Ausgabegeschwindigkeit (tok/s) / Cache-Treffer
- **Performance-Matrix**: Erfolgsquote, Dauer, Geschwindigkeit und Cache-Trefferquote aller Modelle auf einen Blick, sortiert nach der gewählten Metrik
- **Request-Log**: Eingabe / neue Eingabe / Ausgabe / Cache-Lesen / Dauer / TTFT / Geschwindigkeit pro Anfrage, mit Modell- und Statusfiltern sowie Seitenwechsel
- **Cache-Trefferquote**: `Treffer / (Treffer + neue Eingabe)`, der tatsächliche Nutzen von Prefix-Caching
- **Kosten und Abdeckung**: Kosten pro Anfrage aus der Preistabelle plus Anteil der bepreisten Anfragen; Modelle ohne Preis werden als solche gekennzeichnet

---

### 🧾 Anfragedetails und Aufbewahrung

- **Eine Zeile pro Anfrage**: eingehender Pfad, Forward-URL und Upstream-Modell je Versuch, Request-Header, Request-/Response-Body, Token, TTFT, Cache, Kosten und Credits; Retries und Backend-Wechsel sind einzeln sichtbar
- **Keine sensiblen Werte in der Datenbank**: Header werden nur über eine Allowlist erfasst; `Authorization` / `x-api-key` / `Cookie` sind immer ausgeschlossen; Zugangsdaten in Query-Parametern der Forward-URL (`key` / `token` / `sig`, ...) werden maskiert
- **Body-Erfassung ist opt-in**: Request- und Response-Body haben getrennte Schalter mit 64-KB-Limit pro Eintrag; Streams speichern rohes Upstream-SSE, und Cross-Protocol-Anfragen speichern die rohe Upstream-Antwort statt des konvertierten Ergebnisses
- **Aufbewahrung**: aus oder 30/90/180/365 Tage; läuft beim Start und einmal täglich. Minimum 30 Tage — kürzer würde Nutzung löschen, die monatliche Key-Quotas noch zählen

### 🎟️ Upstream-Credit-Budgets

- **Plattform-Budget + Credit-Preis pro Modell**: Plattformen können ein natürliches Tages-/Wochen-/Monatsbudget setzen (z. B. ModelScope 250 Credits/Tag), Modelle einen Preis pro Anfrage (1 oder 2 Credits)
- **Credit-bewusstes Routing**: Plattformen mit zu wenig Guthaben werden übersprungen; sind alle erschöpft, antwortet der Gateway mit 429 und der nächsten Reset-Zeit, statt still ein Konto über dem Limit zu nutzen
- **Getrenntes Ledger**: Aufladungen, Freigaben, Anpassungen und Resets haben Ledger-Zeilen; das Löschen der Anfragehistorie „erstattet“ bereits verbrauchte Credits nicht
- **Sichtbar und anpassbar**: Die Anfragehistorie zeigt die Credits pro Anfrage; Plattformliste/-detail zeigt Rest/Gesamt und ungepreiste Modelle; niedrige/erschöpfte Credits landen im Benachrichtigungszentrum und lassen sich manuell anpassen

## 🧱 Bekannte Einschränkungen

Das sind Implementierungsgrenzen, bei denen man leicht zu viel annimmt:

1. **„Latency based" ist Round Robin**: Die Option existiert, die Auswahl nutzt aber weiterhin Round Robin, und die Latenz fließt nicht in die Reihenfolge ein (`src/lb/mod.rs`).
2. **„Priorität" degradiert bei Fehlern nicht**: Es wird immer das aktive Backend mit dem kleinsten `priority`-Wert verwendet; ein Retry wählt es erneut, Backends mit niedrigerer Priorität bleiben untätig.
3. **Keine Auslistung per Health-Check**: Konnektivitätsprüfungen der Plattform dienen nur Anzeige und Alarmen und verändern das Routing nicht; das Feld `fallback` der Route ist nicht in den Anfragepfad eingebunden.
4. **Verbindungsfehler werden standardmäßig nicht wiederholt**: Standardmäßig werden nur 429 / 5xx / 408 wiederholt; Netzwerk-Timeouts und Dial-Fehler zählen als `ConnectionError` und stehen nicht auf der Retry-Liste.
5. **Auch ohne konfigurierte Schlüssel muss ein Schlüssel mitgeschickt werden**: Sind keine API-Schlüssel vorhanden, prüft das Gateway den Wert nicht, aber die Anfrage muss trotzdem einen `Authorization`- oder `x-api-key`-Header tragen (der Wert wird nicht geprüft); sobald ein Schlüssel existiert, ist ein gültiger Schlüssel erforderlich.
6. **„Least Connections" endet mit den Antwortheadern**: Der Zähler wird freigegeben, sobald die Antwortheader des Upstreams eintreffen; er deckt nicht die Laufzeit eines Streaming-Bodys ab und ist kein vollständiger Streaming-Nebenläufigkeitszähler.
7. **Request-/Response-Bodys sind standardmäßig aus**: zwei getrennte Schalter mit 64-KB-Limit pro Eintrag; Response-Details speichern die rohe Upstream-Antwort, Streams nur die ersten 64 KB, und Cross-Protocol-Anfragen sind nicht das konvertierte Ergebnis des Clients.
8. **Auto-Purge dauert mindestens 30 Tage**: ist er aus, wird manuell gelöscht; Löschen gibt nur SQLite-Seiten frei und verkleinert die Datei nicht (separat `VACUUM` ausführen).
9. **Credits sind ein lokales Ledger**: bei Erfolg reserviert/bestätigt, bei Fehler freigegeben, manuell anpassbar; Fenster nutzen den lokalen natürlichen Tag/Woche/Monat des Servers und können von der Upstream-Rechnung abweichen.

---

## 🆚 Vergleich

| | AI Gateway | Nginx-Reverse-Proxy | One API | LiteLLM |
|---|---|---|---|---|
| **Form** | Desktop-App + Single-Binary-Server | Allzweck-Reverse-Proxy | Server | Python-Dienst |
| **Visuelle Verwaltung** | ✅ Integrierte Konsole | ❌ | ✅ Web-Konsole | ⚠️ Admin-UI muss aktiviert werden |
| **Lastverteilung** | ✅ 4 wirksame Strategien + Latenzoption | ✅ Upstream-Round-Robin / Hashing | ⚠️ Einfach | ✅ |
| **OpenAI-Protokoll** | ✅ | ✅ Durchreichen | ✅ | ✅ |
| **Anthropic-Protokoll** | ✅ Nativ | ❌ Nur Durchreichen, keine Konvertierung | ✅ Konvertierung | ✅ Konvertierung |
| **Wiederholungen bei Fehlern** | ✅ 429 / 5xx / 408 | ⚠️ Erfordert `proxy_next_upstream` | ✅ | ✅ Retries + Fallbacks |
| **Remote-Modellliste** | ✅ Wird beim Verbindungstest abgerufen | ❌ | ❌ | ⚠️ Konfigurationsgesteuert |
| **Desktop-App** | ✅ macOS / Windows / Linux | ❌ | ❌ | ❌ |
| **Implementierung** | Rust | C | Go | Python |

> Basierend auf der öffentlichen Dokumentation und der Standardform der jeweiligen Projekte; Details bitte beim jeweiligen Projekt prüfen.

---

## ✨ Weitere Funktionen

- 🏪 **15 Plattform-Presets**: OpenAI · Anthropic · DeepSeek · NVIDIA · Moonshot · ZhipuAI · Qwen · Doubao · SiliconFlow · Groq · Google Gemini · Together AI · OpenRouter · Ollama · Azure OpenAI. Presets füllen Base-URL und Modellliste aus; den Schlüssel lieferst du weiterhin selbst. Jeder OpenAI- / Anthropic-kompatible Endpunkt kann auch manuell angelegt werden
- 🔍 **Remote-Modellsynchronisierung**: „Verbindung testen" ruft die Upstream-Modellliste ab; die Synchronisierung ergänzt nur lokal fehlende Modelle und überschreibt vorhandene Preise, Fähigkeiten oder Anzeigenamen nicht
- 🏷️ **Fähigkeits-Badges**: Reasoning / Vision / Embedding / Rerank / Tools, pro Modell und in Massen bearbeitbar; die Badges eines virtuellen Modells sind die Schnittmenge der Fähigkeiten seiner Backends
- 🔗 **reasoning_content-Unterstützung**: Chain-of-Thought-Ausgaben von Plattformen wie NVIDIA NIM; OpenAI- / Anthropic-Reasoning-Parameter werden in beide Richtungen übersetzt
- 📡 **Benutzerdefinierte weitergeleitete Header**: beliebige Header pro Plattform anhängen, die `Authorization` / `x-api-key` namensgleich überschreiben
- 🎛️ **Playground**: Modell und Protokoll wählen, eine echte Anfrage senden, Routing und Upstream prüfen, einen temporären Schlüssel erzeugen oder das passende cURL kopieren
- 🔌 **Client-Zugriff**: Übernahme und Rollback der lokalen Konfigurationen von Claude Code / Codex / OpenCode / OpenClaw / Pi / Hermes mit einem Klick; nur von AI Gateway verwaltete Felder werden geändert, vor jedem Anwenden wird gesichert, inklusive Diff-Vorschau und Rohkonfigurations-Editor
- 🌙 **Themes Hell / Dunkel / System**; UI auf Chinesisch und Englisch
- 🧭 **Acht Seiten**: Traffic · Monitoring · Virtuelle Modelle · Plattformen · API-Schlüssel · Playground · Client-Zugriff · Einstellungen. Auf der Traffic-Seite stehen Filter, Seitenwechsel und Zeiträume in der URL, sodass Aktualisieren und Vor-/Zurücknavigieren den Zustand behalten
- 🧾 **Versuchsverfolgung und Replay**: Jeder Backend-Versuch einer Anfrage wird einzeln erfasst; die Aufbewahrung von Anfrageinhalten ist standardmäßig aus, und nach dem Einschalten kann eine gespeicherte Anfrage wortgleich erneut gesendet werden
- 🔔 **Benachrichtigungszentrum**: ablaufende Schlüssel, wiederholte Plattformfehler, Kontingentüberschreitungen und veraltete Backups werden gespeichert und sind über die Glocke im Kopfbereich erreichbar
- 💰 **Kostenschätzung und Budgets**: Kosten pro Anfrage aus der Preistabelle mit Abdeckungsquote; Schlüssel-Kontingente unterstützen rollierende Fenster und Ablehnung bei Überschreitung
- 🔑 **Kontingente pro Schlüssel**: Ablaufdatum, Anfrage-Limit, Token-Limit, Kostenbudget und Fenster (lebenslang / Tag / Woche / Monat) pro Schlüssel, optional an ein virtuelles Modell gebunden
- 🗄️ **Datenbanksicherung und -wiederherstellung**: Snapshot, Download und Löschen; vor dem Wiederherstellen an Ort und Stelle wird zusätzlich ein Sicherheits-Snapshot angelegt
- 📤 **Konfiguration und CSV**: Der Konfigurationsexport enthält standardmäßig keine Upstream-Geheimnisse (bei Bedarf ausdrücklich aktivierbar) und zeigt vor dem Import eine Vorschau; Traffic lässt sich mit den aktiven Filtern als CSV exportieren, Plattformpreise als CSV importieren und exportieren
- ⚡ **Live-Aktualisierung**: Neue Anfragen und Alarme kommen per SSE, bei Verbindungsabbruch wird auf 30-Sekunden-Polling zurückgefallen
- 🖥️ **Das Schließen des Desktop-Fensters beendet das Gateway nicht**: Es wird nur die WebView zerstört; beim Wiederherstellen aus dem Tray kehrt die zuletzt geöffnete Seite zurück

---

## 🔐 Authentifizierung

Das Dashboard (Traffic und Statistiken, virtuelle Modelle, Plattformen, API-Schlüssel, Playground, Client-Zugriff, Einstellungen) erfordert eine Anmeldung:

- **Erster Start**: Das Dashboard öffnet „Administratorkonto anlegen". Die Ersteinrichtung akzeptiert nur Anfragen vom lokalen Rechner (127.0.0.1), damit kein Teilnehmer im LAN das Konto zuerst übernehmen kann
- **Sitzungen**: Gespeichert in einem HttpOnly-Cookie mit SameSite=Strict, 7 Tage gültig. Abmelden über das Benutzermenü oben rechts
- **Passwort ändern**: Einstellungen → Kontosicherheit. Andere Geräte werden sofort abgemeldet; das aktuelle Gerät erhält eine neue Sitzung
- **Passwort vergessen**: Anwendung stoppen, die Zeilen in den Tabellen `users` und `sessions` löschen und neu starten, um die Einrichtung erneut auszuführen

| Pfad | Authentifizierung |
|---|---|
| `/v1/chat/completions`, `/v1/completions`, `/v1/messages` | Gateway-API-Schlüssel (`Authorization: Bearer ...` oder `x-api-key`) |
| `/v1/models` | Öffentlich |
| `/health` | Öffentlich |
| `/api/auth/status`, `/api/auth/login`, `/api/auth/setup` | Öffentlich (`setup` ist faktisch nur lokal möglich) |
| Alle übrigen `/api/*` (Plattformen / Modelle / virtuelle Modelle / API-Schlüssel / Client-Zugriff / Einstellungen / Statistiken / Backups) | Admin-Sitzung |

> Ohne konfigurierte API-Schlüssel prüft das Gateway den Wert nicht, aber Anfragen müssen trotzdem einen `Authorization`- oder `x-api-key`-Header tragen (der Wert wird nicht geprüft); sobald ein Schlüssel existiert, ist ein gültiger Schlüssel erforderlich. Zugangsdaten liegen in SQLite (Argon2id-Hashes); `admin_token` in `config.toml` ist ein historisches reserviertes Feld.

---

## 🚀 Schnellstart

### Option 1: Desktop-App herunterladen (empfohlen)

Lade das Installationspaket für deine Plattform von [Releases](https://github.com/keiskeies/ai-gateway/releases) herunter. Die Desktop-App gibt es für macOS (arm64 / x64), Windows x64 und Linux x64.

### Option 2: Aus dem Quellcode bauen

```bash
git clone https://github.com/keiskeies/ai-gateway.git
cd ai-gateway

# Zuerst das Frontend bauen; die Ausgabe geht nach ../static/
cd frontend && npm install && npm run build
cd ..

# Eigenständiger Server
cargo run
# http://localhost:1994 aufrufen

# Oder die Tauri-Desktop-App
cargo install tauri-cli
cargo tauri dev
```

> Der Server-Modus liefert `static/` direkt aus, daher muss das Frontend vor dem Start aus dem Quellcode gebaut werden. Der Frontend-Build leert `static/`.

### Vier Schritte

```
0️⃣ Administratorkonto anlegen → anmelden
1️⃣ Plattform hinzufügen → Preset wählen + API-Schlüssel eintragen → speichern
2️⃣ Virtuelles Modell anlegen → Strategie wählen + Backends hinzufügen → aktivieren
3️⃣ base_url ändern → auf http://localhost:1994/v1 zeigen lassen → fertig
```

Um mehrere Schlüssel zu bündeln: Schritt 1 für jeden Schlüssel desselben Anbieters wiederholen und dann alle diese Plattformen in Schritt 2 zum selben virtuellen Modell hinzufügen.

---

## 📖 Verwendung

```python
# OpenAI SDK — nur base_url ändern
from openai import OpenAI
client = OpenAI(
    base_url="http://localhost:1994/v1",  # ← nur diese Zeile ändern
    api_key="your-gateway-key"            # ← im Gateway angelegter API-Schlüssel
)
response = client.chat.completions.create(
    model="your-virtual-model",           # ← Name des virtuellen Modells
    messages=[{"role": "user", "content": "hello"}]
)

# Anthropic SDK — gleiches Prinzip
import anthropic
client = anthropic.Anthropic(
    base_url="http://localhost:1994",     # ← nur diese Zeile ändern (das SDK hängt /v1/messages an)
    api_key="your-gateway-key"
)
response = client.messages.create(
    model="your-virtual-model",
    max_tokens=1024,
    messages=[{"role": "user", "content": "hello"}]
)
```

<details>
<summary>🔧 cURL-Beispiele</summary>

```bash
# OpenAI-kompatibel
curl http://localhost:1994/v1/chat/completions \
  -H "Authorization: Bearer YOUR_GATEWAY_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Anthropic-kompatibel
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_GATEWAY_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# Liste der virtuellen Modelle (öffentlich)
curl http://localhost:1994/v1/models
```

</details>

---

## 🎯 Typische Szenarien

| Szenario | Vorgehen | Ergebnis |
|----------|----------|----------|
| **Kostenlose Kontingente kombinieren** | Eine Plattform pro Konto anlegen, alle in ein virtuelles Modell hängen, Round Robin verwenden | Der Durchsatz summiert sich innerhalb der Limits jedes Schlüssels |
| **Kosten steuern** | Hochvolumige Arbeit auf ein virtuelles Modell mit günstigerem Modell zeigen lassen, bei Bedarf auf ein teureres wechseln | Teure Modelle bearbeiten nur die Anfragen, die sie brauchen |
| **Stabilität** | Mehrere Backends hinter ein virtuelles Modell legen und Wiederholungen bei 429 / 5xx / 408 aktivieren | Temporäre Rate-Limits und Upstream-Aussetzer werden abgefangen |
| **Plattformen mischen** | Je ein virtuelles Modell für OpenAI, Claude und Qwen | Ein SDK und eine Base-URL für alle |
| **Im Team teilen** | Ein Gateway plus Kontingente und Modellbindung pro Schlüssel | Upstream-Zugangsdaten bleiben privat; Nutzung ist nachvollziehbar |

---

## 🏗️ Architektur

```
┌──────────────────────────────────────────────────┐
│                    AI Gateway                     │
│                                                  │
│  ┌──────────┐  ┌───────────┐  ┌──────────────┐  │
│  │  OpenAI  │  │ Anthropic │  │  Admin Web   │  │
│  │ Endpunkt │  │ Endpunkt  │  │   Konsole    │  │
│  └─────┬────┘  └─────┬─────┘  └──────────────┘  │
│        │              │                           │
│  ┌─────▼──────────────▼─────┐                     │
│  │ Konvertierung · Routing ·│                     │
│  │ Wiederholungen           │                     │
│  └──┬────┬────┬────┬────┬──┘                     │
│     │    │    │    │    │                         │
│  ┌──▼─┐┌─▼──┐┌▼───┐┌▼──┐┌▼────┐                │
│  │Deep││Qwen││GLM ││GPT││Claude│  ← N Backends  │
│  │Seek││    ││    ││-4o││     │                   │
│  └────┘└────┘└────┘└───┘└─────┘                │
└──────────────────────────────────────────────────┘
```

---

## ⚙️ Konfiguration

`config.toml` wird beim ersten Start angelegt; Änderungen in den Einstellungen werden in die Datei zurückgeschrieben. Eine vorhandene Datei wird nie überschrieben.

```toml
[server]
host = "0.0.0.0"
admin_port = 1994
log_level = "info"

[database]
path = "data/ai-gateway.db"

[security]
encrypt_key = "ai-gateway-default-key-change-me"  # bitte ändern
admin_token = ""                                  # historisches reserviertes Feld

[defaults]
lb_strategy = "RoundRobin"
max_retries = 2
retry_backoff_ms = 500
request_timeout_secs = 120
stream_timeout_secs = 1800
test_connection_timeout_secs = 10
capture_request_bodies = false
```

> Änderungen an Port und Bind-Adresse erfordern einen Neustart; andere Einstellungen gelten sofort. `capture_request_bodies` steuert, ob Anfrageinhalte in der Datenbank gespeichert werden, und ist standardmäßig aus.

---

## 🛠️ Technologie-Stack

| Schicht | Technologie |
|---|---|
| Backend | Rust 2021 · Actix-Web 4 · Reqwest |
| Datenbank | SQLite (rusqlite mit `bundled`, kein System-SQLite nötig) · r2d2-Pool |
| Frontend | React 19 · TypeScript 6 · Mantine 7 · Recharts · Vite 8 |
| Desktop | Tauri 2 |

---

## 🗺️ Roadmap

- [x] 📊 Monitoring-Dashboard: Trends je Modell / Request-Log / Cache-Trefferquote
- [x] 🔑 Feingranulare API-Key-Kontingente: Modellbindung · Ablaufdatum · Anfrage- / Token- / Kostenlimits · rollierende Fenster
- [x] 🔔 Plattform-Konnektivitätsprüfungen und Benachrichtigungszentrum (nur Anzeige und Alarme; noch nicht für das Routing)
- [ ] ⏱️ Echte Latenzmessung für „Latency based"
- [ ] 🩺 Gesundheitsprüfungsgesteuerte Auslistung und Prioritäts-Downgrade
- [ ] 🧭 `fallback` der Route in den Anfragepfad einbinden
- [ ] 🌐 Multimodales Gateway (Übersetzungs- / TTS- / Embedding-Endpunkte)
- [ ] 🐳 Docker-Deployment mit einem Befehl
- [ ] ☁️ Cloud-Konfigurationssynchronisierung

> Eine Funktion gewünscht? [Issue öffnen](https://github.com/keiskeies/ai-gateway/issues).

---

## 🤝 Mitwirken

1. Dieses Repo forken
2. Feature-Branch erstellen (`git checkout -b feature/amazing-feature`)
3. Änderungen committen (`git commit -m 'Add some amazing feature'`)
4. Branch pushen (`git push origin feature/amazing-feature`)
5. Pull Request öffnen

---

## 📂 Projektstruktur

```
ai-gateway/
├── src/                  # Rust-Backend (Bibliothek + eigenständiges Binary)
│   ├── api/              # REST- und Admin-APIs
│   ├── db/               # Datenbankschicht (r2d2-Pool)
│   ├── proxy/            # Proxy-Handler, Stream-Konvertierung, Signatur-Cache
│   ├── lb/               # Lastverteilungs-Auswahl
│   ├── protocol/         # OpenAI- / Anthropic-Adapter und Reasoning-Parameter
│   ├── clients/          # Übernahme lokaler Client-Konfigurationen (Claude Code / Codex / …)
│   └── models/           # Datenmodelle
├── frontend/             # React-Frontend (baut nach ../static/)
├── src-tauri/            # Tauri-Desktop-Wrapper
├── e2e/                  # Playwright- + pytest-End-to-End-Tests
├── doc/                  # Screenshots
├── config.toml           # Laufzeitkonfiguration
└── data/                 # SQLite-Datenbank und Backups
```

---

## ⭐ Star-Verlauf

<p align="center">
  <a href="https://star-history.com/#keiskeies/ai-gateway&Date">
    <img src="https://api.star-history.com/svg?repos=keiskeies/ai-gateway&type=Date" alt="Star History Chart" width="600" />
  </a>
</p>

---

## 📜 Lizenz

[MIT License](LICENSE)

---

<p align="center">
  Hilfreich gefunden? Gib dem Projekt einen <a href="https://github.com/keiskeies/ai-gateway">⭐ Star</a>.
</p>
