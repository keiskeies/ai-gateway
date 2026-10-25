<p align="center">
  <img src="logo.png" width="100" alt="AI Gateway Logo" />
</p>

<h1 align="center">🚀 AI Gateway</h1>

<p align="center">
  <strong>Um gateway para todas as plataformas de IA</strong><br/>
  Balanceamento com várias chaves · novas tentativas e troca de backend · protocolos OpenAI &amp; Anthropic · nenhuma mudança no cliente
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
  <a href="README_DE.md">Deutsch</a> ·
  <a href="README_FR.md">Français</a> ·
  <a href="README_ES.md">Español</a> ·
  <strong>Português</strong>
</p>

<p align="center">
  <a href="#-início-rápido"><strong>Início rápido →</strong></a>
</p>

---

> 🌟 Se este projeto ajuda você, uma estrela ajuda outras pessoas a encontrá-lo.

---

## 😩 Que problema resolve

- Você tem várias chaves do mesmo provedor e quer usá-las juntas, sem escrever sua própria rotação e lógica de novas tentativas
- Você quer novas tentativas automáticas em 429 / 5xx / timeouts do upstream, em vez de reimplementá-las em cada cliente
- OpenAI e Anthropic significam dois SDKs, duas URLs base e dois formatos de mensagem
- Camadas gratuitas e modelos pagos se misturam, enquanto custos, cotas e causas de falha ficam espalhados
- Você quer distribuir chaves para uma equipe sem entregar as credenciais dos provedores

O AI Gateway reúne tudo isso em um processo local: **um ponto de entrada, um conjunto de chaves, um conjunto de estatísticas**.

---

## 📸 Capturas de tela

Todas as capturas são do aplicativo desktop atual (tema escuro).

<p align="center">
  <img src="doc/console-wall.png" width="92%" alt="Painel de monitoramento" />
</p>
<p align="center"><em>Monitoramento — faixa de KPIs e tendências por modelo em oito métricas (requisições / falhas / entrada / saída / latência / TTFT / velocidade / cache) em granularidade de hora, dia, mês e ano, mapa de calor de atividade (7d / 30d / 12m / Tudo), participação por plataforma, matriz de desempenho e registro de requisições. Tela cheia e pausa da atualização são suportadas</em></p>

<details>
<summary>📱 Mostrar as demais páginas</summary>

<p align="center">
  <img src="doc/console-traffic.png" width="92%" alt="Tráfego" />
</p>
<p align="center"><em>Tráfego — um resumo de «precisa de atenção» para limites de taxa, falhas e chaves inativas; a tabela filtra por modelo, plataforma, chave, status, tipo de erro e faixa de latência, com paginação; o painel de detalhe mostra a cadeia de tentativas (status e duração de cada tentativa de backend), barras de TTFT / geração, leitura de cache versus entrada nova e custo estimado, e permite copiar o ID ou reenviar a requisição</em></p>

<p align="center">
  <img src="doc/console-models.png" width="92%" alt="Modelos virtuais" />
</p>
<p align="center"><em>Modelos virtuais — status, número de backends, estratégia, volume de requisições, taxa de sucesso, latência média e custo diretamente na lista</em></p>

<p align="center">
  <img src="doc/console-model-routing.png" width="92%" alt="Roteamento do modelo" />
</p>
<p align="center"><em>Detalhe do modelo · Roteamento — edite estratégia, tentativas, backoff, max_tokens padrão e distribuição de pesos à esquerda; a espinha de roteamento desenha os backends como um grafo ponderado e ordenado por prioridade no centro; inspecione à direita o provedor, os selos de capacidade e o tratamento dos parâmetros de raciocínio de um backend</em></p>

<p align="center">
  <img src="doc/console-platforms.png" width="92%" alt="Plataformas" />
</p>
<p align="center"><em>Plataformas — resultado do teste de conectividade, número de modelos sincronizados, quantos modelos virtuais a referenciam, volume de requisições e taxa de sucesso; plataformas degradadas são contadas no cabeçalho</em></p>

<p align="center">
  <img src="doc/console-platform-detail.png" width="92%" alt="Detalhe da plataforma" />
</p>
<p align="center"><em>Detalhe da plataforma — credenciais e cabeçalhos repassados (chaves salvas são retornadas apenas mascaradas, nunca em texto claro), busca de modelos remotos via «Testar conexão», importação seletiva de modelos da plataforma, importação/exportação de preços em CSV, e quais modelos virtuais dependem dessa plataforma</em></p>

<p align="center">
  <img src="doc/console-keys.png" width="92%" alt="Chaves de API" />
</p>
<p align="center"><em>Chaves de API — requisições, tokens, latência média, custo e último uso por chave; uma chave pode ser vinculada a um único modelo virtual</em></p>

<p align="center">
  <img src="doc/console-key-quota.png" width="92%" alt="Cota da chave" />
</p>
<p align="center"><em>Cota da chave — expiração, limite de requisições, limite de tokens, orçamento de custo e janela de cota (vitalícia / dia / semana / mês). Atingir qualquer limite retorna 429; o orçamento de custo só se aplica a modelos com preço</em></p>

<p align="center">
  <img src="doc/console-playground.png" width="92%" alt="Playground" />
</p>
<p align="center"><em>Playground — escolha um modelo virtual e um protocolo (OpenAI / Anthropic) e envie uma requisição real para verificar o roteamento e o upstream; crie uma chave temporária na hora ou copie o cURL equivalente</em></p>

<p align="center">
  <img src="doc/console-clients.png" width="92%" alt="Acesso de clientes" />
</p>
<p align="center"><em>Acesso de clientes — assunção por cartões das configurações locais de Claude Code / Codex / OpenCode / OpenClaw / Pi / Hermes; ao expandir o diff aparece, cada aplicação é copiada para backup e a desconexão restaura o estado</em></p>

<p align="center">
  <img src="doc/console-settings.png" width="92%" alt="Configurações" />
</p>
<p align="center"><em>Configurações — porta e endereço de escuta, padrões da política de requisições, segurança da conta, estatísticas de dados (linhas / trilhas de tentativas / alertas / linhas órfãs / retenção de corpos de requisição), exportação e importação de configuração, e backup e restauração completos do banco</em></p>

</details>

---

## ⚡ Funcionalidades principais

### 🎯 Várias chaves: combine várias chaves de um mesmo provedor

Uma plataforma armazena uma única credencial upstream; um modelo virtual tem um ou mais backends. Para combinar várias chaves:

```
Criar uma plataforma por chave → adicionar todas ao mesmo modelo virtual → escolher rodízio ou menos conexões
```

```
Sua aplicação
   │
   └──→ Modelo virtual ──┬── Plataforma A (chave 1, 3 RPM)
                         ├── Plataforma B (chave 2, 3 RPM)
                         └── Plataforma C (chave 3, 3 RPM)
```

A vazão total é a soma dos limites de cada chave; o gateway não contorna os limites de taxa do upstream.

### 🔀 Estratégias de balanceamento

| Estratégia | Comportamento | Ideal para |
|-----------|---------------|------------|
| **Rodízio (round robin)** | Distribui as requisições em turnos entre os backends do modelo virtual | Combinar chaves / contas |
| **Aleatório ponderado** | Escolhe um backend conforme `weight` | Capacidade desigual dos backends |
| **Menos conexões** | Escolhe o backend com menos requisições aguardando os cabeçalhos de resposta do upstream | Suavizar picos de requisições curtas |
| **Prioridade** | Sempre o backend ativo com o menor valor de `priority` | Um backend principal fixo |

> «Latência» está disponível na interface, mas hoje usa a implementação de rodízio; a latência ainda não influencia a seleção. «Prioridade» também não faz downgrade automático quando o backend principal falha. Veja [Limitações conhecidas](#-limitações-conhecidas).

### 🌐 Protocolos OpenAI e Anthropic

```
SDK OpenAI / ferramentas compatíveis com OpenAI ──→ POST /v1/chat/completions ──┐
                                                                                ├──→ roteamento → upstream
SDK Anthropic ───────────────────────────────────→ POST /v1/messages ──────────┘
```

| Caminho | Protocolo | Observação |
|---|---|---|
| `/v1/chat/completions` | OpenAI | Entrada principal |
| `/v1/completions` | OpenAI | Caminho antigo, interpretado como chat |
| `/v1/messages` | Anthropic | Formato nativo do Anthropic |
| `/v1/models` | OpenAI | Público; retorna modelos virtuais e capacidades |

Respostas sem streaming do mesmo protocolo são repassadas sem alteração (apenas `model` é substituído pelo nome do modelo virtual); respostas entre protocolos são reconstruídas a partir do formato unificado, e o streaming é convertido bloco a bloco.

### 🧠 Raciocínio: o cliente decide

Quaisquer parâmetros de raciocínio enviados pelo cliente são traduzidos para o formato que o upstream entende. Se o cliente não enviar nenhum, o gateway não ativa o raciocínio por conta própria.

| O cliente envia | O upstream é Claude | O upstream é um modelo de raciocínio compatível com OpenAI |
|---|---|---|
| `reasoning_effort: "high"` (OpenAI) | `thinking: {type: "enabled", budget_tokens: 16384}` | `reasoning_effort: "high"` |
| `thinking: {budget_tokens: 10000}` (Anthropic) | Mantido; o orçamento é reduzido se `max_tokens` for pequeno demais | Mapeado para o esforço correspondente |

- Estilo por backend: `Auto` / OpenAI effort / Anthropic thinking / OpenRouter / Qwen / GLM / vLLM Qwen / desativado
- `Auto` resolve pelo tipo de upstream: `thinking` para Anthropic, `reasoning_effort` nos demais; `param_overrides` é mesclado por último e prevalece
- Assinaturas do Anthropic são reproduzidas sem alteração; loops de ferramentas entre protocolos usam um cache de assinaturas em memória (30 minutos, indexado por `tool_use.id`), e o raciocínio é desativado em vez de fabricar uma assinatura quando não há correspondência
- `/v1/models` expõe `capabilities` / `supported_parameters` / `reasoning` para modelos de raciocínio; exibir um controle de raciocínio ainda depende da tabela de capacidades do próprio cliente

### 🛡️ Novas tentativas e troca de backend

- Novas tentativas por rota: `max_retries` e `backoff_ms` são ajustáveis na interface; os tipos de erro repetidos são fixos em 429 / 5xx / 408
- Por padrão, **429 / 5xx / 408 (respostas de timeout do upstream)** são repetidos com backoff exponencial (500 ms × 2ⁿ)
- Cada tentativa passa novamente pelo balanceador; no rodízio, normalmente cai no próximo backend
- Requisições em streaming usam um `stream_timeout_secs` separado e mais longo (1800 s por padrão), para que gerações longas não sejam cortadas pelo timeout normal

> Erros de conexão (backend inalcançável, falha de conexão, timeout de rede) não são repetidos por padrão, e não há remoção guiada por health-check. Se você precisa de «trocar automaticamente quando um backend cai», coloque os backends em um mesmo modelo virtual com rodízio / menos conexões, ou desative manualmente o backend com falha. Veja [Limitações conhecidas](#-limitações-conhecidas).

### 📊 Monitoramento

Um perfil completo de desempenho para cada modelo em granularidade de hora / dia / mês / ano:

- **Oito tendências**: requisições / falhas / entrada / saída / latência / TTFT / velocidade de saída (tok/s) / acertos de cache
- **Matriz de desempenho**: taxa de sucesso, duração, velocidade e taxa de cache de todos os modelos de uma vez, ordenados pela métrica escolhida
- **Registro de requisições**: entrada / entrada nova / saída / leitura de cache / duração / TTFT / velocidade por requisição, com filtros por modelo e status e paginação
- **Taxa de cache**: `acertos / (acertos + entrada nova)`, o benefício real do cache de prefixo
- **Custo e cobertura**: custo por requisição a partir da tabela de preços e proporção de requisições com preço; modelos sem preço são sinalizados como tal

---

### 🧾 Detalhes das requisições e retenção

- **Uma linha por requisição**: caminho de entrada, URL de encaminhamento e modelo upstream por tentativa, cabeçalhos, corpo da requisição/resposta, tokens, TTFT, cache, custo e créditos; retries e trocas de backend aparecem um a um
- **Nenhum valor sensível no banco**: os cabeçalhos são capturados por uma lista de permissões; `Authorization` / `x-api-key` / `Cookie` são sempre excluídos; credenciais nos parâmetros da URL de encaminhamento (`key` / `token` / `sig`, ...) são mascaradas
- **A captura de corpos é opt-in**: requisição e resposta têm chaves separadas, com limite de 64 KB por registro; streams guardam o SSE upstream bruto, e requisições entre protocolos guardam a resposta upstream bruta, não o resultado convertido
- **Retenção**: desligada ou 30/90/180/365 dias; roda na inicialização e uma vez por dia. O mínimo é 30 dias — menos apagaria uso que as cotas mensais das chaves ainda contam

### 🎟️ Orçamentos de créditos upstream

- **Orçamento da plataforma + preço em créditos por modelo**: a plataforma pode definir um orçamento natural diário/semanal/mensal (por exemplo, ModelScope 250 créditos/dia), e o modelo um preço por requisição (1 ou 2 créditos)
- **Roteamento ciente de créditos**: plataformas com saldo insuficiente são ignoradas; quando todas esgotam, o gateway retorna 429 com o próximo horário de reset em vez de usar silenciosamente uma conta acima do limite
- **Livro-razão separado**: cobranças, liberações, ajustes e resets têm linha própria; limpar o histórico de requisições nunca “devolve” créditos já consumidos
- **Visível e ajustável**: o log de requisições mostra os créditos de cada uma; a lista/detalhe da plataforma mostra restante/total e modelos sem preço; créditos baixos/esgotados vão para a central de notificações e podem ser ajustados manualmente

## 🧱 Limitações conhecidas

Estes são limites de implementação fáceis de superestimar:

1. **«Latência» é rodízio**: a opção existe, mas a seleção continua usando rodízio e a latência não entra na ordenação (`src/lb/mod.rs`).
2. **«Prioridade» não faz downgrade em falha**: sempre usa o backend ativo com o menor valor de `priority`; uma nova tentativa o escolhe de novo, e backends de prioridade menor ficam ociosos.
3. **Sem remoção guiada por health-check**: as verificações de conectividade servem apenas para exibição e alertas e não alteram o roteamento; o campo `fallback` da rota não está ligado ao caminho da requisição.
4. **Erros de conexão não são repetidos por padrão**: apenas 429 / 5xx / 408; timeouts de rede e falhas de conexão contam como `ConnectionError` e não estão na lista.
5. **Uma chave ainda é necessária mesmo sem chaves configuradas**: sem nenhuma chave de API, o gateway não valida o valor, mas a requisição ainda precisa levar um cabeçalho `Authorization` ou `x-api-key` (o valor não é validado); assim que uma chave existe, uma chave válida é exigida.
6. **«Menos conexões» termina nos cabeçalhos de resposta**: o contador é liberado assim que os cabeçalhos do upstream chegam, então não cobre a duração de um corpo em streaming e não é uma contagem real de concorrência de streaming.
7. **Os corpos de requisição/resposta vêm desligados**: dois interruptores separados com limite de 64 KB por registro; os detalhes de resposta guardam a resposta upstream bruta, streams apenas os primeiros 64 KB, e requisições entre protocolos não são o resultado convertido que o cliente viu.
8. **O auto-purge dura pelo menos 30 dias**: desligado, a limpeza é manual; apagar só libera páginas do SQLite e não reduz o arquivo (execute `VACUUM` separadamente).
9. **Créditos são um livro-razão local**: reservados/confirmados em caso de sucesso e liberados em caso de falha, com ajuste manual; as janelas usam o dia/semana/mês natural local do servidor e podem divergir da fatura upstream.

---

## 🆚 Comparação

| | AI Gateway | Proxy reverso Nginx | One API | LiteLLM |
|---|---|---|---|---|
| **Forma** | App desktop + servidor binário único | Proxy reverso de uso geral | Servidor | Serviço Python |
| **Gestão visual** | ✅ Console integrado | ❌ | ✅ Console web | ⚠️ Admin UI precisa ser ativada |
| **Balanceamento de carga** | ✅ 4 estratégias efetivas + opção de latência | ✅ Rodízio / hash de upstream | ⚠️ Básico | ✅ |
| **Protocolo OpenAI** | ✅ | ✅ Repasse | ✅ | ✅ |
| **Protocolo Anthropic** | ✅ Nativo | ❌ Apenas repasse, sem conversão | ✅ Conversão | ✅ Conversão |
| **Novas tentativas em falhas** | ✅ 429 / 5xx / 408 | ⚠️ Requer `proxy_next_upstream` | ✅ | ✅ Retries + fallbacks |
| **Lista remota de modelos** | ✅ Obtida ao testar a conexão | ❌ | ❌ | ⚠️ Conforme configuração |
| **App desktop** | ✅ macOS / Windows / Linux | ❌ | ❌ | ❌ |
| **Implementação** | Rust | C | Go | Python |

> Baseado na documentação pública e na forma padrão de cada projeto; consulte o projeto correspondente para detalhes.

---

## ✨ Mais recursos

- 🏪 **15 presets de plataforma**: OpenAI · Anthropic · DeepSeek · NVIDIA · Moonshot · ZhipuAI · Qwen · Doubao · SiliconFlow · Groq · Google Gemini · Together AI · OpenRouter · Ollama · Azure OpenAI. Os presets preenchem a URL base e a lista de modelos; a chave continua sendo sua. Qualquer endpoint compatível com OpenAI / Anthropic também pode ser adicionado manualmente
- 🔍 **Sincronização de modelos remotos**: «Testar conexão» busca a lista de modelos do upstream; a sincronização só adiciona modelos ausentes localmente e não sobrescreve preços, capacidades ou nomes existentes
- 🏷️ **Selos de capacidade**: raciocínio / visão / embedding / rerank / ferramentas, editáveis por modelo e em lote; os selos de um modelo virtual são a interseção das capacidades dos seus backends
- 🔗 **Suporte a reasoning_content**: saídas de cadeia de pensamento de plataformas como NVIDIA NIM; parâmetros de raciocínio OpenAI / Anthropic traduzidos nos dois sentidos
- 📡 **Cabeçalhos repassados personalizados**: adicione cabeçalhos arbitrários por plataforma, sobrescrevendo `Authorization` / `x-api-key` quando os nomes coincidem
- 🎛️ **Playground**: escolha um modelo e um protocolo, envie uma requisição real para verificar o roteamento e o upstream, crie uma chave temporária ou copie o cURL equivalente
- 🔌 **Acesso de clientes**: assumir e restaurar com um clique as configurações locais de Claude Code / Codex / OpenCode / OpenClaw / Pi / Hermes; apenas os campos gerenciados pelo AI Gateway são alterados, cada aplicação é copiada para backup, com prévia do diff e editor de configuração bruta
- 🌙 **Temas claro / escuro / sistema**; interface em chinês e inglês
- 🧭 **Oito páginas**: Tráfego · Monitoramento · Modelos virtuais · Plataformas · Chaves de API · Playground · Acesso de clientes · Configurações. Na página Tráfego, filtros, paginação e faixas de tempo vivem na URL, então atualizar e navegar para frente/trás preservam o estado
- 🧾 **Trilhas de tentativas e replay**: cada tentativa de backend de uma requisição é registrada; a retenção de corpos de requisição fica desligada por padrão e, quando ativada, uma requisição armazenada pode ser reenviada palavra por palavra
- 🔔 **Central de notificações**: chaves expirando, falhas repetidas de plataformas, estouros de cota e backups desatualizados são persistidos e acessíveis pelo sino no cabeçalho
- 💰 **Estimativa de custos e orçamentos**: custo por requisição a partir da tabela de preços com taxa de cobertura; as cotas de chave suportam janelas deslizantes e rejeição ao ultrapassar o limite
- 🔑 **Cotas por chave**: expiração, limite de requisições, limite de tokens, orçamento de custo e janela (vitalícia / dia / semana / mês) por chave, opcionalmente vinculada a um modelo virtual
- 🗄️ **Backup e restauração do banco**: snapshot, download e exclusão; restaurar no local mantém antes um snapshot de segurança extra
- 📤 **Configuração e CSV**: a exportação de configuração exclui segredos upstream por padrão (ativação explícita quando necessário) e mostra uma prévia antes de importar; o tráfego é exportado em CSV com os filtros ativos; os preços das plataformas entram e saem em CSV
- ⚡ **Atualização ao vivo**: novas requisições e alertas chegam por SSE, com volta para polling de 30 segundos quando a conexão cai
- 🖥️ **Fechar a janela do desktop não encerra o gateway**: apenas a WebView é destruída; restaurar pela bandeja volta à página que estava aberta

---

## 🔐 Autenticação

O painel (tráfego e estatísticas, modelos virtuais, plataformas, chaves de API, Playground, acesso de clientes, configurações) exige login:

- **Primeiro início**: o painel abre «Criar conta de administrador». A inicialização só aceita requisições da máquina local (127.0.0.1), para que alguém na LAN não reivindique a conta primeiro
- **Sessões**: guardadas em cookie HttpOnly SameSite=Strict, válido por 7 dias. Saia pelo menu de usuário no canto superior direito
- **Trocar a senha**: Configurações → Segurança da conta. Os outros dispositivos são desconectados imediatamente; o dispositivo atual recebe uma nova sessão
- **Esqueceu a senha**: pare o aplicativo, apague as linhas das tabelas `users` e `sessions` e reinicie para executar a inicialização novamente

| Caminho | Autenticação |
|---|---|
| `/v1/chat/completions`, `/v1/completions`, `/v1/messages` | Chave de API do gateway (`Authorization: Bearer ...` ou `x-api-key`) |
| `/v1/models` | Público |
| `/health` | Público |
| `/api/auth/status`, `/api/auth/login`, `/api/auth/setup` | Público (`setup` é, na prática, restrito à máquina local) |
| Todos os outros `/api/*` (plataformas / modelos / modelos virtuais / chaves de API / acesso de clientes / configurações / estatísticas / backups) | Sessão de administrador |

> Sem nenhuma chave de API configurada, o gateway não valida o valor, mas as requisições ainda precisam levar um cabeçalho `Authorization` ou `x-api-key` (o valor não é validado); assim que uma chave existe, uma chave válida é exigida. As credenciais ficam no SQLite (hashes Argon2id); `admin_token` em `config.toml` é um campo histórico reservado.

---

## 🚀 Início rápido

### Opção 1: baixar o app desktop (recomendado)

Baixe o instalador da sua plataforma em [Releases](https://github.com/keiskeies/ai-gateway/releases). O app desktop está disponível para macOS (arm64 / x64), Windows x64 e Linux x64.

### Opção 2: compilar a partir do código-fonte

```bash
git clone https://github.com/keiskeies/ai-gateway.git
cd ai-gateway

# Compile primeiro o frontend; a saída vai para ../static/
cd frontend && npm install && npm run build
cd ..

# Servidor autônomo
cargo run
# Abra http://localhost:1994

# Ou o app desktop Tauri
cargo install tauri-cli
cargo tauri dev
```

> O modo servidor serve `static/` diretamente, então compile o frontend antes de executar a partir do código-fonte. A compilação do frontend limpa `static/`.

### Quatro passos

```
0️⃣ Criar a conta de administrador → entrar
1️⃣ Adicionar uma plataforma → escolher preset + informar a chave de API → salvar
2️⃣ Criar um modelo virtual → escolher estratégia + adicionar backends → ativar
3️⃣ Alterar base_url → apontar para http://localhost:1994/v1 → pronto
```

Para combinar várias chaves: repita o passo 1 para cada chave do mesmo provedor e depois adicione todas essas plataformas ao mesmo modelo virtual no passo 2.

---

## 📖 Uso

```python
# SDK OpenAI — altere apenas base_url
from openai import OpenAI
client = OpenAI(
    base_url="http://localhost:1994/v1",  # ← altere esta linha
    api_key="your-gateway-key"            # ← chave de API criada no gateway
)
response = client.chat.completions.create(
    model="your-virtual-model",           # ← nome do modelo virtual
    messages=[{"role": "user", "content": "hello"}]
)

# SDK Anthropic — mesma ideia
import anthropic
client = anthropic.Anthropic(
    base_url="http://localhost:1994",     # ← altere esta linha (o SDK acrescenta /v1/messages)
    api_key="your-gateway-key"
)
response = client.messages.create(
    model="your-virtual-model",
    max_tokens=1024,
    messages=[{"role": "user", "content": "hello"}]
)
```

<details>
<summary>🔧 Exemplos com cURL</summary>

```bash
# Compatível com OpenAI
curl http://localhost:1994/v1/chat/completions \
  -H "Authorization: Bearer YOUR_GATEWAY_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Compatível com Anthropic
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_GATEWAY_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# Lista de modelos virtuais (pública)
curl http://localhost:1994/v1/models
```

</details>

---

## 🎯 Cenários típicos

| Cenário | Como | Resultado |
|---------|------|-----------|
| **Combinar camadas gratuitas** | Uma plataforma por conta, todas em um modelo virtual, rodízio | A vazão se soma dentro do limite de cada chave |
| **Controlar custos** | Direcionar o volume alto para um modelo virtual apoiado por um modelo mais barato e trocar para o premium quando necessário | Modelos caros só atendem as requisições que precisam deles |
| **Estabilidade** | Vários backends atrás de um modelo virtual e novas tentativas em 429 / 5xx / 408 | Limites temporários e oscilações do upstream são absorvidos |
| **Misturar plataformas** | Um modelo virtual para OpenAI, um para Claude e um para Qwen | Um único SDK e uma única URL base para todos |
| **Compartilhar com a equipe** | Um gateway mais cotas e vínculo de modelo por chave | As credenciais upstream ficam privadas; o uso é rastreável |

---

## 🏗️ Arquitetura

```
┌──────────────────────────────────────────────────┐
│                    AI Gateway                     │
│                                                  │
│  ┌──────────┐  ┌───────────┐  ┌──────────────┐  │
│  │  OpenAI  │  │ Anthropic │  │  Admin Web   │  │
│  │ Endpoint │  │ Endpoint  │  │   Console    │  │
│  └─────┬────┘  └─────┬─────┘  └──────────────┘  │
│        │              │                           │
│  ┌─────▼──────────────▼─────┐                     │
│  │ Conversão · Roteamento · │                     │
│  │ Novas tentativas         │                     │
│  └──┬────┬────┬────┬────┬──┘                     │
│     │    │    │    │    │                         │
│  ┌──▼─┐┌─▼──┐┌▼───┐┌▼──┐┌▼────┐                │
│  │Deep││Qwen││GLM ││GPT││Claude│  ← N backends  │
│  │Seek││    ││    ││-4o││     │                   │
│  └────┘└────┘└────┘└───┘└─────┘                │
└──────────────────────────────────────────────────┘
```

---

## ⚙️ Configuração

`config.toml` é criado no primeiro início; alterações feitas nas Configurações são gravadas de volta no arquivo. Um arquivo existente nunca é sobrescrito.

```toml
[server]
host = "0.0.0.0"
admin_port = 1994
log_level = "info"

[database]
path = "data/ai-gateway.db"

[security]
encrypt_key = "ai-gateway-default-key-change-me"  # altere
admin_token = ""                                  # campo histórico reservado

[defaults]
lb_strategy = "RoundRobin"
max_retries = 2
retry_backoff_ms = 500
request_timeout_secs = 120
stream_timeout_secs = 1800
test_connection_timeout_secs = 10
capture_request_bodies = false
```

> Alterações de porta e endereço de escuta exigem reinício; os demais ajustes valem imediatamente. `capture_request_bodies` controla se os corpos de requisição são gravados no banco e fica desligado por padrão.

---

## 🛠️ Stack de tecnologia

| Camada | Tecnologia |
|---|---|
| Backend | Rust 2021 · Actix-Web 4 · Reqwest |
| Banco de dados | SQLite (rusqlite com `bundled`, sem SQLite do sistema) · pool r2d2 |
| Frontend | React 19 · TypeScript 6 · Mantine 7 · Recharts · Vite 8 |
| Desktop | Tauri 2 |

---

## 🗺️ Roadmap

- [x] 📊 Painel de monitoramento: tendências por modelo / registro de requisições / taxa de cache
- [x] 🔑 Cotas refinadas por chave de API: vínculo a modelo · expiração · limites de requisições / tokens / custo · janelas deslizantes
- [x] 🔔 Verificações de conectividade das plataformas e central de notificações (apenas exibição e alertas; ainda não usadas no roteamento)
- [ ] ⏱️ Ligar medição real de latência ao «Latência»
- [ ] 🩺 Remoção guiada por health-check e downgrade de prioridade
- [ ] 🧭 Ligar o campo `fallback` da rota ao caminho da requisição
- [ ] 🌐 Gateway multimodal (endpoints de tradução / TTS / embedding)
- [ ] 🐳 Implantação com Docker em um comando
- [ ] ☁️ Sincronização de configuração na nuvem

> Quer um recurso? [Abra uma issue](https://github.com/keiskeies/ai-gateway/issues).

---

## 🤝 Contribuindo

1. Faça um fork deste repositório
2. Crie uma branch de recurso (`git checkout -b feature/amazing-feature`)
3. Faça commit das alterações (`git commit -m 'Add some amazing feature'`)
4. Envie a branch (`git push origin feature/amazing-feature`)
5. Abra um pull request

---

## 📂 Estrutura do projeto

```
ai-gateway/
├── src/                  # Backend Rust (biblioteca + binário autônomo)
│   ├── api/              # APIs REST e administrativas
│   ├── db/               # Camada de banco de dados (pool r2d2)
│   ├── proxy/            # Handlers de proxy, conversão de streaming, cache de assinaturas
│   ├── lb/               # Seletor de balanceamento de carga
│   ├── protocol/         # Adaptadores OpenAI / Anthropic e parâmetros de raciocínio
│   ├── clients/          # Assunção de configurações locais de clientes (Claude Code / Codex / …)
│   └── models/           # Modelos de dados
├── frontend/             # Frontend React (compila para ../static/)
├── src-tauri/            # Wrapper desktop Tauri
├── e2e/                  # Testes end-to-end com Playwright + pytest
├── doc/                  # Capturas de tela
├── config.toml           # Configuração de execução
└── data/                 # Banco SQLite e backups
```

---

## ⭐ Histórico de estrelas

<p align="center">
  <a href="https://star-history.com/#keiskeies/ai-gateway&Date">
    <img src="https://api.star-history.com/svg?repos=keiskeies/ai-gateway&type=Date" alt="Star History Chart" width="600" />
  </a>
</p>

---

## 📜 Licença

[MIT License](LICENSE)

---

<p align="center">
  Achou útil? Deixe uma <a href="https://github.com/keiskeies/ai-gateway">⭐ estrela</a>.
</p>
