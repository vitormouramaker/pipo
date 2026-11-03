# Pipo

Bichinho virtual de desktop para Windows 11. Uma esfera minimalista, intelectual e com um leve toque de sarcasmo, que flutua na tela e ajuda no dia a dia com atalhos, dicas e tarefas: desenvolvimento, documentos, jogos, lives e notícias.

> Projeto pessoal. Planejamento e visual aprovados; o desenvolvimento ainda não começou.

## Prévia

Abra [`assets/preview/pipo-preview.html`](assets/preview/pipo-preview.html) no navegador para ver o Pipo animado e todos os estados: esfera grafite, olhos geométricos e anel de status, com cor usada só para comunicar estado.

## Planejamento

O plano completo está em [`docs/Pipo - Plano de Implementacao.docx`](docs/). Ele cobre escopo do MVP, stack, arquitetura, funcionalidades por fase, personalidade, privacidade, roadmap e riscos.

## Stack planejada

- **App desktop:** Tauri 2 (Rust)
- **Interface:** React + TypeScript + Vite + Tailwind
- **Personagem:** SVG animado com CSS e Framer Motion
- **IA:** Claude API (Haiku por padrão, Sonnet para tarefas complexas)
- **Dados locais:** SQLite
- **Pacotes:** npm

## Roadmap

| Fase | Entrega |
|---|---|
| 0 | Preparação: repositório, projeto Tauri base, CI |
| 1 | MVP: Pipo flutuante e animado, bandeja, atalho `Ctrl + Alt + Shift + Espaço`, chat com IA |
| 2 | Sensores: janela ativa, modo jogo, hardware, humor reativo |
| 3 | Ações: atalhos de dev, documentos, rotinas, lembretes, OCR |
| 4 | Mundo externo: lives, notícias, briefing matinal |
| 5 | Vida: voz, XP, evolução, acessórios |

## Privacidade

Os dados de contexto ficam no computador. A chave da API nunca entra no repositório: a pasta `api/` está no `.gitignore`, e o app guarda a chave no Gerenciador de Credenciais do Windows.
