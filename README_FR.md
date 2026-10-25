<p align="center">
  <img src="logo.png" width="100" alt="AI Gateway Logo" />
</p>

<h1 align="center">🚀 AI Gateway</h1>

<p align="center">
  <strong>Une passerelle pour toutes les plateformes d'IA</strong><br/>
  Répartition de charge multi-clés · nouvelles tentatives et bascule de backend · protocoles OpenAI &amp; Anthropic · aucune modification côté client
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
  <strong>Français</strong> ·
  <a href="README_ES.md">Español</a> ·
  <a href="README_PT.md">Português</a>
</p>

<p align="center">
  <a href="#-démarrage-rapide"><strong>Démarrage rapide →</strong></a>
</p>

---

> 🌟 Si ce projet vous aide, une étoile aide les autres à le trouver.

---

## 😩 Le problème résolu

- Vous avez plusieurs clés chez un même fournisseur et souhaitez les utiliser ensemble, sans écrire votre propre rotation et votre propre logique de retry
- Vous voulez des nouvelles tentatives automatiques sur 429 / 5xx / timeouts upstream, au lieu de les réimplémenter dans chaque client
- OpenAI et Anthropic, ce sont deux SDK, deux URL de base et deux formats de messages
- Les paliers gratuits et les modèles payants sont mélangés, tandis que les coûts, les quotas et les causes d'échec sont dispersés
- Vous voulez distribuer des clés à une équipe sans partager les identifiants des fournisseurs

AI Gateway réunit tout cela dans un processus local : **un point d'entrée, un jeu de clés, un jeu de statistiques**.

---

## 📸 Captures d'écran

Toutes les captures proviennent de l'application de bureau actuelle (thème sombre).

<p align="center">
  <img src="doc/console-wall.png" width="92%" alt="Supervision" />
</p>
<p align="center"><em>Supervision — bandeau de KPI et tendances par modèle sur huit métriques (requêtes / échecs / entrée / sortie / latence / TTFT / vitesse / cache) en granularité heure, jour, mois et année, carte de chaleur d'activité (7j / 30j / 12m / Tout), répartition par plateforme, matrice de performance et journal des requêtes. Le plein écran et la pause de l'actualisation sont pris en charge</em></p>

<details>
<summary>📱 Afficher les autres pages</summary>

<p align="center">
  <img src="doc/console-traffic.png" width="92%" alt="Trafic" />
</p>
<p align="center"><em>Trafic — un résumé « nécessite attention » pour les limites de débit, les échecs et les clés inactives ; filtre des requêtes par modèle, plateforme, clé, statut, type d'erreur et plage de latence, avec pagination ; le volet de détail montre la chaîne des tentatives (statut et durée de chaque essai de backend), les barres TTFT / génération, la lecture du cache face à l'entrée nouvelle et le coût estimé, et permet de copier l'ID ou de rejouer la requête</em></p>

<p align="center">
  <img src="doc/console-models.png" width="92%" alt="Modèles virtuels" />
</p>
<p align="center"><em>Modèles virtuels — état, nombre de backends, stratégie, volume de requêtes, taux de réussite, latence moyenne et coût directement dans la liste</em></p>

<p align="center">
  <img src="doc/console-model-routing.png" width="92%" alt="Routage du modèle" />
</p>
<p align="center"><em>Détail du modèle · Routage — modifiez la stratégie, les tentatives, le backoff, le max_tokens par défaut et la distribution des poids à gauche ; la colonne vertébrale du routage dessine les backends sous forme de graphe pondéré et ordonné par priorité au centre ; inspectez à droite le fournisseur, les badges de capacités et le traitement des paramètres de raisonnement d'un backend</em></p>

<p align="center">
  <img src="doc/console-platforms.png" width="92%" alt="Plateformes" />
</p>
<p align="center"><em>Plateformes — résultat du test de connectivité, nombre de modèles synchronisés, nombre de modèles virtuels qui la référencent, volume de requêtes et taux de réussite ; les plateformes dégradées sont comptées dans l'en-tête</em></p>

<p align="center">
  <img src="doc/console-platform-detail.png" width="92%" alt="Détail de la plateforme" />
</p>
<p align="center"><em>Détail de la plateforme — identifiants et en-têtes transférés (les clés enregistrées ne sont renvoyées que sous forme masquée, jamais en clair), récupération des modèles distants via « Tester la connexion », import sélectif de modèles depuis la plateforme, import/export CSV des prix, et la liste des modèles virtuels qui dépendent de cette plateforme</em></p>

<p align="center">
  <img src="doc/console-keys.png" width="92%" alt="Clés API" />
</p>
<p align="center"><em>Clés API — requêtes, tokens, latence moyenne, coût et date de dernière utilisation par clé ; une clé peut être rattachée à un seul modèle virtuel</em></p>

<p align="center">
  <img src="doc/console-key-quota.png" width="92%" alt="Quota de clé" />
</p>
<p align="center"><em>Quota de clé — expiration, limite de requêtes, limite de tokens, budget de coût et fenêtre de quota (vie entière / jour / semaine / mois). Atteindre une limite renvoie 429 ; le budget de coût ne s'applique qu'aux modèles tarifés</em></p>

<p align="center">
  <img src="doc/console-playground.png" width="92%" alt="Playground" />
</p>
<p align="center"><em>Playground — choisissez un modèle virtuel et un protocole (OpenAI / Anthropic), envoyez une vraie requête pour vérifier le routage et l'upstream ; créez une clé temporaire sur place ou copiez le cURL équivalent</em></p>

<p align="center">
  <img src="doc/console-clients.png" width="92%" alt="Accès clients" />
</p>
<p align="center"><em>Accès clients — prise en charge par cartes des configurations locales de Claude Code / Codex / OpenCode / OpenClaw / Pi / Hermes ; le dépliage affiche le diff, chaque application est sauvegardée et la déconnexion restaure l'état</em></p>

<p align="center">
  <img src="doc/console-settings.png" width="92%" alt="Réglages" />
</p>
<p align="center"><em>Réglages — port et adresse d'écoute, valeurs par défaut de la politique de requête, sécurité du compte, statistiques de données (lignes / traces de tentatives / alertes / lignes orphelines / rétention des corps de requête), export et import de configuration, sauvegarde et restauration complètes de la base</em></p>

</details>

---

## ⚡ Fonctionnalités clés

### 🎯 Plusieurs clés : regrouper plusieurs clés d'un même fournisseur

Une plateforme stocke un seul identifiant upstream ; un modèle virtuel possède un ou plusieurs backends. Pour combiner plusieurs clés :

```
Créer une plateforme par clé → les rattacher toutes au même modèle virtuel → choisir tourniquet ou moins de connexions
```

```
Votre application
   │
   └──→ Modèle virtuel ──┬── Plateforme A (clé 1, 3 RPM)
                          ├── Plateforme B (clé 2, 3 RPM)
                          └── Plateforme C (clé 3, 3 RPM)
```

Le débit total est la somme des limites de chaque clé ; la passerelle ne contourne pas les limites de débit du fournisseur.

### 🔀 Stratégies de répartition de charge

| Stratégie | Comportement | Idéale pour |
|-----------|-------------|-------------|
| **Tourniquet (round robin)** | Répartit les requêtes à tour de rôle entre les backends du modèle virtuel | Combiner des clés / comptes |
| **Aléatoire pondéré** | Choisit un backend selon `weight` | Capacités inégales des backends |
| **Moins de connexions** | Choisit le backend avec le moins de requêtes en attente des en-têtes de réponse upstream | Lisser les pics de requêtes courtes |
| **Priorité** | Choisit toujours le backend actif avec la plus petite valeur `priority` | Un backend principal fixe |

> « Latence » est proposé dans l'interface mais utilise encore l'implémentation du tourniquet ; la latence n'entre pas encore dans la sélection. « Priorité » ne bascule pas non plus automatiquement en cas d'échec du backend principal. Voir [Limites connues](#-limites-connues).

### 🌐 Protocoles OpenAI et Anthropic

```
SDK OpenAI / outils compatibles OpenAI ──→ POST /v1/chat/completions ──┐
                                                                       ├──→ routage → upstream
SDK Anthropic ──────────────────────────→ POST /v1/messages ──────────┘
```

| Chemin | Protocole | Remarque |
|---|---|---|
| `/v1/chat/completions` | OpenAI | Point d'entrée principal |
| `/v1/completions` | OpenAI | Ancien chemin, analysé comme du chat |
| `/v1/messages` | Anthropic | Format Anthropic natif |
| `/v1/models` | OpenAI | Public ; renvoie les modèles virtuels et les capacités |

Les réponses non-streaming de même protocole sont transmises telles quelles (seul `model` est remplacé par le nom du modèle virtuel) ; les réponses inter-protocoles sont reconstruites à partir du format unifié, et les réponses en streaming sont converties bloc par bloc.

### 🧠 Raisonnement : c'est le client qui décide

Quels que soient les paramètres de raisonnement envoyés par le client, la passerelle les traduit dans le format compris par l'upstream. Si le client n'en envoie aucun, la passerelle n'active pas le raisonnement d'elle-même.

| Le client envoie | L'upstream est Claude | L'upstream est un modèle de raisonnement compatible OpenAI |
|---|---|---|
| `reasoning_effort: "high"` (OpenAI) | `thinking: {type: "enabled", budget_tokens: 16384}` | `reasoning_effort: "high"` |
| `thinking: {budget_tokens: 10000}` (Anthropic) | Conservé tel quel ; le budget est réduit si `max_tokens` est trop petit | Converti en effort correspondant |

- Style par backend : `Auto` / OpenAI effort / Anthropic thinking / OpenRouter / Qwen / GLM / vLLM Qwen / désactivé
- `Auto` résout selon le type d'upstream : `thinking` pour Anthropic, `reasoning_effort` sinon ; `param_overrides` est fusionné en dernier et gagne
- Les signatures Anthropic sont rejouées à l'identique ; les boucles d'outils inter-protocoles utilisent un cache de signatures en mémoire (30 minutes, indexé par `tool_use.id`), et le raisonnement est désactivé plutôt que de fabriquer une signature en cas d'absence dans le cache
- `/v1/models` expose `capabilities` / `supported_parameters` / `reasoning` pour les modèles de raisonnement ; l'affichage d'un contrôle de raisonnement dépend toujours de la table de capacités propre au client

### 🛡️ Nouvelles tentatives et bascule de backend

- Nouvelles tentatives par route : `max_retries` et `backoff_ms` sont réglables dans l'interface ; les types d'erreurs réessayés sont fixés à 429 / 5xx / 408
- Par défaut, **429 / 5xx / 408 (réponses de timeout upstream)** sont réessayés avec un backoff exponentiel (500 ms × 2ⁿ)
- Chaque tentative repasse par le répartiteur de charge ; en tourniquet, elle arrive généralement sur le backend suivant
- Les requêtes en streaming utilisent un `stream_timeout_secs` distinct et plus long (1800 s par défaut), afin que les générations longues ne soient pas coupées par le timeout normal

> Les erreurs de connexion (backend injoignable, échec de connexion, timeout réseau) ne sont pas réessayées par défaut, et il n'y a pas d'éjection pilotée par health-check. Si vous avez besoin d'une « bascule automatique en cas de panne », placez les backends dans un même modèle virtuel derrière un tourniquet / moins de connexions, ou désactivez manuellement le backend défaillant. Voir [Limites connues](#-limites-connues).

### 📊 Supervision

Un profil de performance complet pour chaque modèle en granularité heure / jour / mois / année :

- **Huit tendances** : requêtes / échecs / entrée / sortie / latence / TTFT / vitesse de sortie (tok/s) / cache
- **Matrice de performance** : taux de réussite, durée, vitesse et taux de cache de tous les modèles en un coup d'œil, triés selon la métrique choisie
- **Journal des requêtes** : entrée / entrée nouvelle / sortie / lecture du cache / durée / TTFT / vitesse par requête, avec filtres par modèle et statut et pagination
- **Taux de cache** : `hits / (hits + entrée nouvelle)`, le bénéfice réel du cache de préfixe
- **Coût et couverture** : coût par requête à partir de la table de prix et part des requêtes tarifées ; les modèles sans prix sont signalés comme tels

---

### 🧾 Détails des requêtes et rétention

- **Une ligne par requête** : chemin entrant, URL de transfert et modèle amont par tentative, en-têtes, corps de requête/réponse, tokens, TTFT, cache, coût et crédits ; les retries et changements de backend sont visibles un par un
- **Aucune valeur sensible en base** : les en-têtes sont capturés via une liste blanche ; `Authorization` / `x-api-key` / `Cookie` sont toujours exclus ; les identifiants dans les paramètres de l'URL de transfert (`key` / `token` / `sig`, ...) sont masqués
- **La capture des corps est opt-in** : les corps de requête et de réponse ont des interrupteurs séparés, avec une limite de 64 Ko par entrée ; les flux stockent le SSE amont brut, et les requêtes inter-protocoles stockent la réponse amont brute plutôt que le résultat converti
- **Rétention** : désactivée ou 30/90/180/365 jours ; s'exécute au démarrage et une fois par jour. Le minimum est de 30 jours — moins supprimerait de l'usage encore compté par les quotas mensuels des clés

### 🎟️ Budgets de crédits amont

- **Budget de plateforme + prix en crédits par modèle** : une plateforme peut définir un budget naturel jour/semaine/mois (par exemple ModelScope 250 crédits/jour), et un modèle un prix par requête (1 ou 2 crédits)
- **Routage conscient des crédits** : les plateformes sans solde suffisant sont ignorées ; quand toutes sont épuisées, la passerelle renvoie 429 avec la prochaine date de réinitialisation au lieu d'utiliser silencieusement un compte dépassé
- **Registre séparé** : débits, libérations, ajustements et réinitialisations ont chacun une ligne ; purger l'historique des requêtes ne « rembourse » jamais les crédits déjà consommés
- **Visible et ajustable** : le journal des requêtes affiche les crédits par requête ; la liste/fiche plateforme affiche restant/total et le nombre de modèles non tarifés ; les crédits faibles/épuisés vont au centre de notifications et peuvent être ajustés manuellement

## 🧱 Limites connues

Voici les limites d'implémentation qu'il est facile de surestimer :

1. **« Latence » est un tourniquet** : l'option existe, mais la sélection utilise toujours le tourniquet et la latence n'entre pas dans l'ordre (`src/lb/mod.rs`).
2. **« Priorité » ne se dégrade pas en cas d'échec** : elle utilise toujours le backend actif avec la plus petite valeur `priority` ; un retry le choisit à nouveau et les backends de priorité inférieure restent inactifs.
3. **Pas d'éjection pilotée par health-check** : les tests de connectivité servent uniquement à l'affichage et aux alertes et ne modifient pas le routage ; le champ `fallback` de la route n'est pas branché dans le chemin de requête.
4. **Les erreurs de connexion ne sont pas réessayées par défaut** : seuls 429 / 5xx / 408 le sont ; les timeouts réseau et les échecs de connexion relèvent de `ConnectionError` et ne figurent pas dans la liste.
5. **Une clé reste requise même sans clé configurée** : sans aucune clé API, la passerelle ne valide pas la valeur, mais la requête doit tout de même porter un en-tête `Authorization` ou `x-api-key` (la valeur n'est pas validée) ; dès qu'une clé existe, une clé valide est requise.
6. **« Moins de connexions » s'arrête aux en-têtes de réponse** : le compteur est libéré dès l'arrivée des en-têtes upstream ; il ne couvre pas la durée d'un corps en streaming et ne représente donc pas la concurrence réelle du streaming.
7. **Les corps de requête/réponse sont désactivés par défaut** : deux interrupteurs séparés avec une limite de 64 Ko par entrée ; les détails de réponse stockent la réponse amont brute, les flux seulement les 64 premiers Ko, et les requêtes inter-protocoles ne sont pas le résultat converti vu par le client.
8. **L'auto-purge dure au moins 30 jours** : désactivé, la purge est manuelle ; la suppression ne libère que des pages SQLite et ne réduit pas le fichier (exécuter `VACUUM` séparément).
9. **Les crédits sont un registre local** : réservés/confirmés en cas de succès et libérés en cas d'échec, avec ajustement manuel ; les fenêtres utilisent le jour/semaine/mois naturel local du serveur et peuvent différer de la facture amont.

---

## 🆚 Comparaison

| | AI Gateway | Reverse proxy Nginx | One API | LiteLLM |
|---|---|---|---|---|
| **Forme** | Application de bureau + serveur mono-binaire | Reverse proxy généraliste | Serveur | Service Python |
| **Gestion visuelle** | ✅ Console intégrée | ❌ | ✅ Console web | ⚠️ Admin UI à activer |
| **Répartition de charge** | ✅ 4 stratégies effectives + option latence | ✅ Round robin / hachage upstream | ⚠️ Basique | ✅ |
| **Protocole OpenAI** | ✅ | ✅ Transmission | ✅ | ✅ |
| **Protocole Anthropic** | ✅ Natif | ❌ Transmission seule, sans conversion | ✅ Conversion | ✅ Conversion |
| **Nouvelles tentatives** | ✅ 429 / 5xx / 408 | ⚠️ Nécessite `proxy_next_upstream` | ✅ | ✅ Retries + fallbacks |
| **Liste de modèles distante** | ✅ Récupérée lors du test de connexion | ❌ | ❌ | ⚠️ Pilotée par la configuration |
| **Application de bureau** | ✅ macOS / Windows / Linux | ❌ | ❌ | ❌ |
| **Implémentation** | Rust | C | Go | Python |

> Établi à partir de la documentation publique et de la forme par défaut de chaque projet ; vérifiez auprès du projet concerné.

---

## ✨ Autres fonctionnalités

- 🏪 **15 presets de plateformes** : OpenAI · Anthropic · DeepSeek · NVIDIA · Moonshot · ZhipuAI · Qwen · Doubao · SiliconFlow · Groq · Google Gemini · Together AI · OpenRouter · Ollama · Azure OpenAI. Les presets remplissent l'URL de base et la liste des modèles ; vous fournissez toujours la clé. Tout endpoint compatible OpenAI / Anthropic peut aussi être ajouté à la main
- 🔍 **Synchronisation des modèles distants** : « Tester la connexion » récupère la liste des modèles de l'upstream ; la synchronisation n'ajoute que les modèles absents en local et n'écrase ni les prix, ni les capacités, ni les noms d'affichage existants
- 🏷️ **Badges de capacités** : raisonnement / vision / embedding / rerank / outils, modifiables par modèle et en masse ; les badges d'un modèle virtuel sont l'intersection des capacités de ses backends
- 🔗 **Prise en charge de reasoning_content** : sorties de chaîne de pensée de plateformes comme NVIDIA NIM ; paramètres de raisonnement OpenAI / Anthropic traduits dans les deux sens
- 📡 **En-têtes transférés personnalisés** : ajoutez des en-têtes arbitraires par plateforme, en écrasant `Authorization` / `x-api-key` à nom égal
- 🎛️ **Playground** : choisissez un modèle et un protocole, envoyez une vraie requête pour vérifier le routage et l'upstream, créez une clé temporaire ou copiez le cURL équivalent
- 🔌 **Accès clients** : prise en charge et restauration en un clic des configurations locales de Claude Code / Codex / OpenCode / OpenClaw / Pi / Hermes ; seuls les champs gérés par AI Gateway sont modifiés, chaque application est sauvegardée, avec aperçu du diff et éditeur de configuration brute
- 🌙 **Thèmes clair / sombre / système** ; interface en chinois et en anglais
- 🧭 **Huit pages** : Trafic · Supervision · Modèles virtuels · Plateformes · Clés API · Playground · Accès clients · Réglages. Sur la page Trafic, les filtres, la pagination et les plages de temps vivent dans l'URL : actualiser et naviguer avant/arrière conservent l'état
- 🧾 **Traces de tentatives et rejeu** : chaque essai de backend d'une requête est enregistré ; la rétention des corps de requête est désactivée par défaut, et une fois activée, une requête stockée peut être rejouée mot pour mot
- 🔔 **Centre de notifications** : clés qui expirent, échecs répétés de plateformes, dépassements de quota et sauvegardes obsolètes sont persistés et accessibles depuis la cloche de l'en-tête
- 💰 **Estimation des coûts et budgets** : coût par requête à partir de la table de prix avec un taux de couverture ; les quotas de clé prennent en charge les fenêtres glissantes et le rejet en cas de dépassement
- 🔑 **Quotas par clé** : expiration, limite de requêtes, limite de tokens, budget de coût et fenêtre (vie entière / jour / semaine / mois) par clé, éventuellement rattachée à un modèle virtuel
- 🗄️ **Sauvegarde et restauration de la base** : snapshot, téléchargement et suppression ; une restauration sur place conserve d'abord un snapshot de sécurité supplémentaire
- 📤 **Configuration et CSV** : l'export de configuration exclut les secrets upstream par défaut (activation explicite si besoin), avec aperçu avant import ; le trafic s'exporte en CSV selon les filtres actifs ; les prix des plateformes s'importent et s'exportent en CSV
- ⚡ **Mises à jour en direct** : les nouvelles requêtes et alertes arrivent en SSE, avec retour à un polling de 30 secondes si la connexion tombe
- 🖥️ **Fermer la fenêtre de bureau n'arrête pas la passerelle** : seule la WebView est détruite ; la restauration depuis la zone de notification revient à la page qui était ouverte

---

## 🔐 Authentification

Le tableau de bord (trafic et statistiques, modèles virtuels, plateformes, clés API, Playground, accès clients, réglages) exige une connexion :

- **Premier lancement** : le tableau de bord ouvre « Créer le compte administrateur ». L'initialisation n'accepte que les requêtes de la machine locale (127.0.0.1), afin qu'un pair du LAN ne puisse pas revendiquer le compte en premier
- **Sessions** : stockées dans un cookie HttpOnly SameSite=Strict, valable 7 jours. Déconnexion via le menu utilisateur en haut à droite
- **Changer le mot de passe** : Réglages → Sécurité du compte. Les autres appareils sont déconnectés immédiatement ; l'appareil courant reçoit une nouvelle session
- **Mot de passe oublié** : arrêtez l'application, supprimez les lignes des tables `users` et `sessions`, puis redémarrez pour relancer l'initialisation

| Chemin | Authentification |
|---|---|
| `/v1/chat/completions`, `/v1/completions`, `/v1/messages` | Clé API de la passerelle (`Authorization: Bearer ...` ou `x-api-key`) |
| `/v1/models` | Public |
| `/health` | Public |
| `/api/auth/status`, `/api/auth/login`, `/api/auth/setup` | Public (`setup` de fait limité à la machine locale) |
| Tous les autres `/api/*` (plateformes / modèles / modèles virtuels / clés API / accès clients / réglages / statistiques / sauvegardes) | Session administrateur |

> Sans aucune clé API configurée, la passerelle ne valide pas la valeur, mais les requêtes doivent tout de même porter un en-tête `Authorization` ou `x-api-key` (la valeur n'est pas validée) ; dès qu'une clé existe, une clé valide est requise. Les identifiants sont stockés dans SQLite (hachages Argon2id) ; `admin_token` dans `config.toml` est un champ historique réservé.

---

## 🚀 Démarrage rapide

### Option 1 : télécharger l'application de bureau (recommandé)

Récupérez l'installeur de votre plateforme depuis [Releases](https://github.com/keiskeies/ai-gateway/releases). L'application de bureau est disponible pour macOS (arm64 / x64), Windows x64 et Linux x64.

### Option 2 : compiler depuis les sources

```bash
git clone https://github.com/keiskeies/ai-gateway.git
cd ai-gateway

# Construire d'abord le frontend ; la sortie va dans ../static/
cd frontend && npm install && npm run build
cd ..

# Serveur autonome
cargo run
# Ouvrir http://localhost:1994

# Ou l'application de bureau Tauri
cargo install tauri-cli
cargo tauri dev
```

> Le mode serveur sert directement `static/` ; construisez donc le frontend avant de lancer depuis les sources. La compilation du frontend vide `static/`.

### Quatre étapes

```
0️⃣ Créer le compte administrateur → se connecter
1️⃣ Ajouter une plateforme → choisir un preset + saisir la clé API → enregistrer
2️⃣ Créer un modèle virtuel → choisir une stratégie + ajouter des backends → activer
3️⃣ Changer base_url → pointer vers http://localhost:1994/v1 → terminé
```

Pour combiner plusieurs clés : répétez l'étape 1 pour chaque clé du même fournisseur, puis ajoutez toutes ces plateformes au même modèle virtuel à l'étape 2.

---

## 📖 Utilisation

```python
# SDK OpenAI — changer uniquement base_url
from openai import OpenAI
client = OpenAI(
    base_url="http://localhost:1994/v1",  # ← changer cette ligne
    api_key="your-gateway-key"            # ← clé API créée dans la passerelle
)
response = client.chat.completions.create(
    model="your-virtual-model",           # ← nom du modèle virtuel
    messages=[{"role": "user", "content": "hello"}]
)

# SDK Anthropic — même principe
import anthropic
client = anthropic.Anthropic(
    base_url="http://localhost:1994",     # ← changer cette ligne (le SDK ajoute /v1/messages)
    api_key="your-gateway-key"
)
response = client.messages.create(
    model="your-virtual-model",
    max_tokens=1024,
    messages=[{"role": "user", "content": "hello"}]
)
```

<details>
<summary>🔧 Exemples cURL</summary>

```bash
# Compatible OpenAI
curl http://localhost:1994/v1/chat/completions \
  -H "Authorization: Bearer YOUR_GATEWAY_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Compatible Anthropic
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_GATEWAY_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# Liste des modèles virtuels (public)
curl http://localhost:1994/v1/models
```

</details>

---

## 🎯 Scénarios typiques

| Scénario | Comment | Résultat |
|----------|---------|----------|
| **Combiner des paliers gratuits** | Une plateforme par compte, toutes dans un modèle virtuel, tourniquet | Le débit s'additionne dans la limite de chaque clé |
| **Maîtriser les coûts** | Diriger les gros volumes vers un modèle virtuel adossé à un modèle moins cher, basculer vers un modèle premium au besoin | Les modèles chers ne traitent que les requêtes qui le nécessitent |
| **Stabilité** | Plusieurs backends derrière un modèle virtuel et nouvelles tentatives sur 429 / 5xx / 408 | Les limites temporaires et les hoquets upstream sont absorbés |
| **Mélanger les plateformes** | Un modèle virtuel pour OpenAI, un pour Claude, un pour Qwen | Un seul SDK et une seule URL de base pour tous |
| **Partager avec une équipe** | Une passerelle, des quotas et un rattachement de modèle par clé | Les identifiants upstream restent privés ; l'usage est traçable |

---

## 🏗️ Architecture

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
│  │ Conversion · Routage ·   │                     │
│  │ Nouvelles tentatives     │                     │
│  └──┬────┬────┬────┬────┬──┘                     │
│     │    │    │    │    │                         │
│  ┌──▼─┐┌─▼──┐┌▼───┐┌▼──┐┌▼────┐                │
│  │Deep││Qwen││GLM ││GPT││Claude│  ← N backends  │
│  │Seek││    ││    ││-4o││     │                   │
│  └────┘└────┘└────┘└───┘└─────┘                │
└──────────────────────────────────────────────────┘
```

---

## ⚙️ Configuration

`config.toml` est créé au premier lancement ; les modifications faites dans les réglages sont réécrites dans ce fichier. Un fichier existant n'est jamais écrasé.

```toml
[server]
host = "0.0.0.0"
admin_port = 1994
log_level = "info"

[database]
path = "data/ai-gateway.db"

[security]
encrypt_key = "ai-gateway-default-key-change-me"  # à modifier
admin_token = ""                                  # champ historique réservé

[defaults]
lb_strategy = "RoundRobin"
max_retries = 2
retry_backoff_ms = 500
request_timeout_secs = 120
stream_timeout_secs = 1800
test_connection_timeout_secs = 10
capture_request_bodies = false
```

> Les changements de port et d'adresse d'écoute nécessitent un redémarrage ; les autres réglages s'appliquent immédiatement. `capture_request_bodies` contrôle le stockage des corps de requête en base et est désactivé par défaut.

---

## 🛠️ Stack technique

| Couche | Technologie |
|---|---|
| Backend | Rust 2021 · Actix-Web 4 · Reqwest |
| Base de données | SQLite (rusqlite avec `bundled`, pas de SQLite système requis) · pool r2d2 |
| Frontend | React 19 · TypeScript 6 · Mantine 7 · Recharts · Vite 8 |
| Bureau | Tauri 2 |

---

## 🗺️ Feuille de route

- [x] 📊 Supervision : tendances par modèle / journal des requêtes / taux de cache
- [x] 🔑 Quotas fins par clé API : rattachement de modèle · expiration · limites requêtes / tokens / coût · fenêtres glissantes
- [x] 🔔 Tests de connectivité des plateformes et centre de notifications (affichage et alertes uniquement ; pas encore utilisés pour le routage)
- [ ] ⏱️ Brancher une vraie mesure de latence sur « Latence »
- [ ] 🩺 Éjection pilotée par health-check et repli de priorité
- [ ] 🧭 Brancher le champ `fallback` de la route dans le chemin de requête
- [ ] 🌐 Passerelle multimodale (points d'entrée traduction / TTS / embedding)
- [ ] 🐳 Déploiement Docker en une commande
- [ ] ☁️ Synchronisation de configuration dans le cloud

> Une fonctionnalité souhaitée ? [Ouvrez une issue](https://github.com/keiskeies/ai-gateway/issues).

---

## 🤝 Contribuer

1. Forkez ce dépôt
2. Créez une branche de fonctionnalité (`git checkout -b feature/amazing-feature`)
3. Commitez vos changements (`git commit -m 'Add some amazing feature'`)
4. Poussez la branche (`git push origin feature/amazing-feature`)
5. Ouvrez une pull request

---

## 📂 Structure du projet

```
ai-gateway/
├── src/                  # Backend Rust (bibliothèque + binaire autonome)
│   ├── api/              # API REST et administration
│   ├── db/               # Couche base de données (pool r2d2)
│   ├── proxy/            # Handlers proxy, conversion de flux, cache de signatures
│   ├── lb/               # Sélecteur de répartition de charge
│   ├── protocol/         # Adaptateurs OpenAI / Anthropic et paramètres de raisonnement
│   ├── clients/          # Prise en charge des configurations client locales (Claude Code / Codex / …)
│   └── models/           # Modèles de données
├── frontend/             # Frontend React (compilé vers ../static/)
├── src-tauri/            # Enveloppe de bureau Tauri
├── e2e/                  # Tests de bout en bout Playwright + pytest
├── doc/                  # Captures d'écran
├── config.toml           # Configuration d'exécution
└── data/                 # Base SQLite et sauvegardes
```

---

## ⭐ Historique des étoiles

<p align="center">
  <a href="https://star-history.com/#keiskeies/ai-gateway&Date">
    <img src="https://api.star-history.com/svg?repos=keiskeies/ai-gateway&type=Date" alt="Star History Chart" width="600" />
  </a>
</p>

---

## 📜 Licence

[MIT License](LICENSE)

---

<p align="center">
  Trouvé utile ? Laissez une <a href="https://github.com/keiskeies/ai-gateway">⭐ étoile</a>.
</p>
