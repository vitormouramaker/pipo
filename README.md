# Pipo

Bichinho virtual de desktop para Windows 11. Um personagem minimalista de óculos redondos, intelectual e com um leve toque de sarcasmo, que flutua na tela e ajuda no dia a dia com atalhos, dicas e tarefas: desenvolvimento, documentos, jogos, lives e notícias.

> Projeto pessoal. Fase 0 concluída: projeto base Tauri, Pipo flutuante na tela e CI gerando instaladores. Próxima etapa: Fase 1 (MVP).

## Prévia

Abra [`assets/preview/pipo-preview.html`](assets/preview/pipo-preview.html) no navegador para ver o Pipo animado e todos os estados: corpo grafite em quadrado arredondado, óculos redondos e olhos geométricos, com cor usada só para comunicar estado.

## Planejamento

O plano completo está em [`docs/Pipo - Plano de Implementacao.docx`](docs/). Ele cobre escopo do MVP, stack, arquitetura, funcionalidades por fase, personalidade, privacidade, roadmap e riscos.

## Como rodar

Pré-requisitos no Windows: Node.js 22+, Rust (stable, via rustup), Visual Studio Build Tools com C++ e WebView2.

```bash
npm install
npm run tauri dev     # abre o Pipo em modo desenvolvimento
npm run tauri build   # gera os instaladores em src-tauri/target/release/bundle/
```

No estado atual (Fase 0), o Pipo aparece como uma esfera flutuante sempre por cima das janelas:

- **arrastar** move o Pipo pela tela;
- **clique** alterna entre os estados de humor;
- **botão direito** fecha o app.

## Estrutura

```
src/              interface (React + TypeScript)
  pipo/Pipo.tsx   personagem em SVG e seus estados
src-tauri/        backend (Rust) e configuração do app
assets/           prévia visual e ícone
docs/             plano de implementação
```

## Stack

- **App desktop:** Tauri 2 (Rust)
- **Interface:** React + TypeScript + Vite + Tailwind
- **Personagem:** SVG animado por código (React + CSS)
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
