# Provedores Futuros e Dificuldades Esperadas

Este documento junta candidatos plausíveis para futuros módulos do `gDownloader`.
O foco aqui não é só "serviço popular", mas sim "serviço que pode virar provider real",
com atenção para:

- suporte a link público de arquivo
- suporte a pasta/coleção quando existir
- estabilidade do link direto
- risco de captcha, token, cookies, sessão ou anti-bot
- possibilidade de `resume` via `Range`
- viabilidade de teste real em CI/local

## Candidatos já mapeados antes

### Drime

- possível dependência forte de API privada/web app
- links podem exigir sessão ou token
- precisa validar suporte a arquivo e pasta

### GoFile

- API relativamente amigável, mas com mudanças frequentes
- pastas, links temporários e expiração precisam ser tratados
- pode haver captcha/rate limit em abuso

### Sendnow

- pouca previsibilidade de formato público
- risco alto de scraping HTML frágil
- precisa validar se há link direto estável

### Terabox

- costuma exigir tokens, cookies e fluxo web mais complexo
- alto risco de anti-bot
- pode exigir autenticação em parte dos fluxos

### 1Fichier

- forte histórico de limites por IP, espera obrigatória e captcha
- downloads gratuitos podem ter cooldown
- pode exigir diferenciação entre usuário anônimo e autenticado

### BRUpload

- suporte provavelmente dependente de scraping HTML
- risco de contadores, espera e links temporários
- precisa validar padrão real de pasta, se existir

## Mais 100 candidatos focados em hosters e serviços comuns em sites de download

| # | Provedor | Perfil | Dificuldades esperadas |
| --- | --- | --- | --- |
| 1 | FreeDL | Hoster / mirror | Pode depender de página intermediária e token curto; validar se há link direto estável. |
| 2 | DailyUploads | Hoster clássico | Risco de countdown, anúncio e link temporário; provável scraping HTML. |
| 3 | Uploady | Hoster clássico | Pode usar botão final de download com token por sessão. |
| 4 | UsersDrive | Hoster clássico | Histórico de páginas intermediárias e possíveis limites para anônimo. |
| 5 | MixDrop | Streaming / file host | Pode exigir resolução de link escondido em JS e headers específicos. |
| 6 | HexUpload | Hoster clássico | Risco de HTML frágil, timer e captcha em abuso. |
| 7 | Clicknupload | Hoster clássico | Pode ter etapas com contagem regressiva e bloqueio por IP. |
| 8 | UploadCloud | Hoster clássico | Links podem expirar rápido; precisa validar `Range` e etapa final. |
| 9 | Racaty | Hoster clássico | Frequentemente aparece em indexadores; provável scraping de página pública. |
| 10 | KatFile | Hoster premium/free | Forte chance de espera, captcha e throttling severo em conta free. |
| 11 | Rapidgator | Hoster premium/free | Muito usado em sites de warez; cooldown, captcha e limite por IP são prováveis. |
| 12 | NitroFlare | Hoster premium/free | Alto valor de mercado, mas host fortemente hostil a anônimo. |
| 13 | Turbobit | Hoster premium/free | Deve exigir etapa de “free download” e possivelmente timer. |
| 14 | Keep2Share | Hoster premium/free | Um dos mais comuns em fóruns; provável foco em premium e limitações para guest. |
| 15 | FileJoker | Hoster premium/free | Pode ter proteção anti-bot e fluxo diferente para premium/free. |
| 16 | DDownload | Hoster premium/free | Bastante usado; risco de countdown, captcha e HTML variável. |
| 17 | UploadGig | Hoster premium/free | Semelhante a DDownload e Rapidgator em fluxo e bloqueios. |
| 18 | FastClick | Hoster / redirector | Pode ser mais redirector do que host final; detectar sem falso positivo é importante. |
| 19 | Send.cm | Hoster clássico | Bom candidato, mas validar links de pasta e estabilidade do arquivo real. |
| 20 | Fikper | Hoster premium/free | Muito presente em fóruns recentes; forte chance de fluxo premium-first. |
| 21 | FileFactory | Hoster premium/free | Hoster antigo, com risco de countdown, páginas de upsell e token. |
| 22 | FileFox | Hoster premium/free | Similar a outros hosters “premium”; depende de teste real de guest mode. |
| 23 | Uploadboy | Hoster clássico | Pode ser focado em público regional; validar link público e fluxo final. |
| 24 | Up-4ever | Hoster clássico | Bastante usado em sites menores; tende a depender de scraping web. |
| 25 | MegaUp | Hoster clássico | Risco de countdown e proteção por sessão. |
| 26 | BayFiles | Hoster clássico | Pode variar bastante conforme espelho e estado atual do serviço. |
| 27 | AnonFiles-like mirrors | Hoster / clones | Muitas instâncias morrem e renascem; idealmente tratar família de clones com cautela. |
| 28 | Uloz.to | Hoster clássico | Serviço grande em alguns mercados; pode ter restrição regional ou login parcial. |
| 29 | FileRio | Hoster clássico | Precisa validar fluxo público atual e presença de redirecionamento pesado. |
| 30 | Drop.download | Hoster clássico | Nome comum em indexadores; risco de HTML volátil e várias páginas. |
| 31 | Upload-4ever | Hoster clássico | Variações de domínio exigem detecção robusta. |
| 32 | ModsFire | Arquivos de mods | Muito comum em mods/games; fluxo tende a ser simples, bom candidato. |
| 33 | GameBanana downloads | Mods / game files | Pode ter etapa de mirror e múltiplas variantes por arquivo. |
| 34 | Nexus Mods CDN/public links | Mods / game files | Em muitos casos exige conta/token; aderência parcial. |
| 35 | CurseForge media | Mods / game files | Pode exigir API ou resolução por página do projeto. |
| 36 | MediaFire mirror clones | Hoster / mirror | Alguns clones mudam só o front-end; validar se vale módulo separado ou regra genérica. |
| 37 | File-upload.com | Hoster clássico | Nome genérico, provável scraping frágil e bloqueio anti-bot. |
| 38 | UploadEE | Hoster clássico | Pode ser alvo bom em nichos, mas precisa validar estado atual. |
| 39 | MirrorAce | Agregador de mirrors | Não é host final; precisa extrair provider real por trás do agregador. |
| 40 | MultiUp | Agregador de mirrors | Muito útil, mas a lógica principal é resolver vários mirrors e escolher um. |
| 41 | Paste-like download pages | Redirector | Alguns sites escondem o host real atrás de página intermediária; detectar sem suporte nativo é delicado. |
| 42 | KrakenFiles | Hoster clássico | Relativamente conhecido; pode ter fluxo mais simples que outros premium hosts. |
| 43 | Qiwi.gg / Qiwi links | Hoster simples | Popular em arquivos e cheats; validar link direto e tempo de vida. |
| 44 | Lumpics / image host downloads | Imagem / mídia | Pode ser útil para nichos, mas foge do foco principal de arquivos grandes. |
| 45 | WorkUpload | Hoster simples | Bem promissor para download direto, precisa validar pasta e `Range`. |
| 46 | EasyUpload | Hoster simples | Pode ter expiração e countdown leve; candidato bom se ainda estiver estável. |
| 47 | UploadNow.io | Hoster simples | Necessário validar se há download direto limpo ou só landing page. |
| 48 | Uploadrar | Hoster clássico | Nome sugere nicho de arquivos compactados; provável HTML legado. |
| 49 | DLFree.fr | Mirror / host | Pode ser relevante em sites franceses; validar geografia e headers. |
| 50 | Desiupload | Hoster clássico | Usado em alguns nichos; provável fluxo premium/free. |
| 51 | Alfafile | Hoster premium/free | Um dos bem conhecidos em fóruns; limites e captcha devem ser altos. |
| 52 | HitFile | Hoster premium/free | Mesmo padrão de hosters premium usados em warez. |
| 53 | TakeFile | Hoster premium/free | Pode compartilhar infraestrutura com outros hosters da mesma rede. |
| 54 | MexaShare | Hoster premium/free | Similar ao ecossistema de hosts com upsell premium. |
| 55 | K2S-like mirrors | Hoster / mirrors | Vários mirrors/rebrands orbitam Keep2Share; parser pode precisar ser por família. |
| 56 | CosmoBox | Hoster clássico | Precisa validar se continua ativo e com guest download viável. |
| 57 | FileSpace | Hoster clássico | Nome recorrente, mas pode conflitar com serviços diferentes; cuidado com detecção. |
| 58 | Downace | Hoster clássico | Pode ser bom para backlog, mas exige confirmação de estabilidade. |
| 59 | Rosefile | Hoster clássico | Presença em fóruns asiáticos e de software; validar padrão público. |
| 60 | Douploads | Hoster clássico | Potencialmente simples, mas pouco previsível sem teste real. |
| 61 | Uploadraja | Hoster clássico | Forte chance de fluxo regional e HTML específico. |
| 62 | MirrorUpload | Agregador / mirror | Semelhante a MultiUp; valor está em resolver host final automaticamente. |
| 63 | MirrorCreator | Agregador / mirror | Mesmo caso: menos provider final, mais resolvedor de mirrors. |
| 64 | DepositFiles | Hoster antigo | Se ainda operar publicamente, deve ter muito legado e fluxo HTML antigo. |
| 65 | Uploaded / Ul.to-like mirrors | Hoster antigo | Muitos domínios históricos mudaram; risco alto de serviço morto ou relançado. |
| 66 | Zippyshare-like clones | Hoster / clones | O original morreu, mas vários clones existem; parser genérico talvez faça mais sentido que módulos separados. |
| 67 | DailyMotion attachments / mirrors | Vídeo / mídia | Baixa prioridade, mas alguns nichos usam pages com asset download. |
| 68 | StreamTape | Streaming / file host | Semelhante a MixDrop; pode esconder o arquivo real em JS inline. |
| 69 | StreamWish | Streaming / file host | Bom para nicho de vídeo; dificuldade parecida com StreamTape. |
| 70 | FileMoon | Streaming / file host | Pode ter forte anti-bot e camada de player antes do arquivo. |
| 71 | VidGuard | Streaming / file host | Mais vídeo que arquivo, mas aparece em ecossistema de mirror/download. |
| 72 | Voe.sx / Voe-like | Streaming / file host | Mesmo desafio: achar asset real sem depender de navegador completo. |
| 73 | doodstream | Streaming / file host | Muito comum em ecossistema de vídeo; fluxos costumam ser hostis a scraping simples. |
| 74 | Uploadrar clones | Hoster / clones | Vale mais como família de sites com o mesmo engine do que provider isolado. |
| 75 | DLUpload | Hoster clássico | Pode ter bom uso em nichos de Android e software. |
| 76 | AndroidFileHost | ROMs / Android | Excelente candidato de nicho, bastante usado em ROMs e kernels. |
| 77 | SourceForge mirror pages | Software / mirrors | Continua bom, porque muitos sites redirecionam para ele. |
| 78 | Fosshub | Software / mirrors | Muito promissor para software público e mirrors limpos. |
| 79 | APKMirror downloads | Android | Pode exigir modelagem específica para múltiplas variantes e páginas intermediárias. |
| 80 | APKPure files | Android | Similar ao APKMirror, com risco de HTML dinâmico. |
| 81 | APKCombo downloads | Android | Nicho Android; útil, mas precisa resolver seleção de variante. |
| 82 | Pling / OpenDesktop files | Linux / themes / apps | Pode ser útil em comunidade Linux; validar assets públicos. |
| 83 | ModDB files | Mods / games | Muito conhecido; fluxo de mirror e seleção de arquivo precisa ser tratado. |
| 84 | IndieDB files | Mods / games | Mesmo padrão do ModDB, porém com menos volume. |
| 85 | Archive.org direct item files | Arquivos públicos | Muito usado como mirror em nichos; bom alvo, mas já mais “clean” que hosters tradicionais. |
| 86 | Catbox | Simples / anônimo | Muito usado para arquivos pequenos e mídia; links costumam ser diretos. |
| 87 | Litterbox | Simples / temporário | Variação temporária do Catbox; expiração precisa ir para a UI. |
| 88 | Pomf-like hosts | Hoster simples | Família de hosts pequenos de upload temporário ou anônimo. |
| 89 | Pomf2 / uguu-like hosts | Hoster simples | Mesma ideia: família de hosts pequenos com link direto e pouca padronização. |
| 90 | Uguu.se | Hoster simples | Muito simples, bom para arquivos pequenos; baixa complexidade. |
| 91 | file.io | Link temporário | Muito usado para envio rápido; expiração e “one-time download” exigem semântica própria. |
| 92 | tmpfiles.org | Temporário | Boa aderência para arquivos rápidos, mas natureza efêmera é central. |
| 93 | Temp.sh | Temporário | Normalmente simples, porém efêmero e focado em link curto. |
| 94 | Pomf.cat | Hoster simples | Outro caso de host pequeno/rápido, muito usado em fóruns e chats. |
| 95 | AnonTransfer | Hoster simples | Baixa complexidade em teoria, mas estado atual precisa ser validado. |
| 96 | Transfer.sh clones | Temporário / CLI-friendly | Muito interessante, sobretudo por link direto, mas há muitas instâncias diferentes. |
| 97 | Oshi.at | Hoster simples | Pode ser um alvo simples e útil, com integração barata. |
| 98 | fileditch | Hoster simples | Aparece em comunidades técnicas; validar disponibilidade e `Range`. |
| 99 | pixeldrain-like mirrors | Hoster simples | Alguns clones/similares podem ser agrupados por família depois. |
| 100 | onedrive short-link wrappers | Wrapper / cloud | Muitos sites de download usam encurtadores em cima de OneDrive; útil resolver o wrapper e cair no provider real. |

## Trilhas recomendadas

### Trilha 1: Alto retorno e boa aderência ao modelo atual

1. GoFile
2. 1Fichier
3. FreeDL
4. OneDrive
5. Proton Drive
6. DailyUploads
7. UsersDrive
8. Clicknupload
9. WorkUpload
10. KrakenFiles

### Trilha 2: Hosters premium/free muito pedidos por sites de download

1. Rapidgator
2. NitroFlare
3. Turbobit
4. Keep2Share
5. FileJoker
6. DDownload
7. UploadGig
8. KatFile
9. AlfaFile
10. Fikper

### Trilha 3: Serviços simples e muito comuns em fóruns, grupos e mirrors

1. Catbox
2. Litterbox
3. Uguu.se
4. tmpfiles.org
5. file.io
6. Temp.sh
7. transfer.sh-like
8. Oshi.at
9. fileditch
10. Pomf-like hosts

### Trilha 4: Clouds e serviços que aparecem muito como mirror secundário

1. OneDrive
2. Proton Drive
3. Dropbox
4. pCloud
5. Box
6. Google Drive
7. MediaFire
8. Mega
9. PixelDrain
10. Nextcloud/public shares

## Requisitos técnicos para qualquer novo provedor

- `detect` confiável
- `get_file_info` para arquivo e pasta quando existir
- download simples
- retry classificado
- suporte a `resume` quando houver `Range`
- extração de erro legível para a UI
- testes reais isolados via `.env` local
- smoke mínimo em macOS, Linux e Windows quando o provider entrar no núcleo suportado

## Regras práticas de priorização

- priorizar hosters que realmente aparecem em indexadores e fóruns de download
- priorizar serviços com modo anônimo/free utilizável antes de serviços premium-only
- separar claramente “host final” de “agregador de mirrors”
- preferir APIs/documentação oficiais quando existirem, mas aceitar scraping quando o valor de mercado do host justificar
- marcar no backlog quais hosters exigem premium, captcha ou cooldown, porque isso muda muito a UX e o valor do módulo
