# Como traduzir o gDownloader

As traduções da interface ficam em `src/renderer/src/locales/*.json`.

Para adicionar ou revisar um idioma:

1. Copie `pt-BR.json` ou `en-US.json` para um novo arquivo no padrão BCP-47, por exemplo `es-ES.json`.
2. Mantenha exatamente as mesmas chaves em todos os arquivos.
3. Traduza apenas os valores.
4. Rode `npm run typecheck` antes de enviar a alteração.

Idiomas ativos hoje: `pt-BR`, `en-US`, `es-ES`, `de-DE`, `fr-FR`, `ru-RU`, `it-IT`, `zh-CN` e `ja-JP`.

Um serviço de comunidade como Crowdin ou Weblate pode sincronizar diretamente essa pasta.
