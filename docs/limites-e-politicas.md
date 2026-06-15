# Limites e Politicas de Download

## Objetivo

Mapear os limites operacionais de cada provedor suportado para orientar:

- deteccao de throttling e bloqueio
- exibicao de estados e mensagens na interface
- estrategia de retry, espera e retomada
- futura automacao de limite de velocidade por host

## Provedores Atuais

| Provedor | Tipos | Limites conhecidos hoje | Sinais de problema | Resposta desejada no app |
| --- | --- | --- | --- | --- |
| Mega | Arquivo e pasta publica | Pode impor cota por IP/tempo e erro temporario em excesso de trafego | resposta da API com erro, queda brusca da taxa, token invalido, 509 em alguns fluxos | mostrar "limite temporario do Mega", pausar host, sugerir retomar depois, permitir retry automatico |
| MediaFire | Arquivo e pasta publica | Pode variar CDN, exigir pagina publica antes do link real e reduzir taxa por sessao/IP | HTML no lugar do binario, redirect quebrado, 403/404/503, arquivo zerado | revalidar link direto, trocar para novo CDN, retry com backoff e log visivel |
| PixelDrain | Arquivo e lista | Pode limitar por abuso ou arquivo removido | 403/404/429, lista sem item valido | exibir indisponivel, retry curto, nao insistir indefinidamente |
| Google Drive | Arquivo publico | Pode exigir confirmacao, quota de compartilhamento ou bloqueio temporario | pagina HTML de confirmacao, 403, "too many users" | diferenciar quota de acesso vs erro generico, orientar usuario e fazer retry controlado |

## Estados de UI Recomendados

- `Disponivel`
- `Lendo metadados`
- `Baixando`
- `Limitado temporariamente`
- `Aguardando retry`
- `Indisponivel`
- `Concluido`
- `Falhou`

## Campos que a UI deve exibir por item

- servidor
- disponibilidade
- tamanho total
- velocidade atual
- percentual
- tempo restante
- quantidade de tentativas usadas
- motivo do bloqueio, quando houver

## Politica tecnica sugerida

### Retry

- falha de rede: retry automatico com backoff exponencial curto
- erro de quota/limite: retry mais lento e com mensagem explicita
- 404/arquivo removido: sem retry automatico longo

### Limite de velocidade

- limite global configuravel
- no futuro: limite por host
- no futuro: janelas por horario

### Observabilidade

- logs por host
- logs por tentativa
- classificacao do erro: rede, autenticacao, quota, indisponivel, parsing

## Lacunas atuais

- falta classificar erros por provedor de forma consistente
- falta pausar automaticamente quando detectar limite
- falta exibir bloqueio/quota por provedor com semantica melhor na lista
- falta separar limite global de limite por host
- falta resumir download parcial por `Range` quando o host suportar

## Implementado nesta fase

- retry configuravel por download
- retry manual e reinicio manual por item
- pause/resume basico
- limitador global aproximado por taxa
- notificacao nativa ao concluir
- fila respeitando limite configuravel de downloads simultaneos
- remocao individual e limpeza de itens finalizados direto no backend da sessao
