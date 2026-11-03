# Pipo

Bichinho virtual de desktop para Windows 11. Um personagem minimalista e tecnológico, intelectual e com um leve toque de sarcasmo, que flutua na tela e ajuda no dia a dia com atalhos, dicas e tarefas: desenvolvimento, documentos, jogos, lives e notícias.

> Projeto pessoal. Fases 1 (MVP) e 2 (sensores) concluídas: Pipo flutuante, chat com a Claude API, gastos por conversa, modo jogo, hardware, área de transferência e humor reativo. Próxima etapa: Fase 3 (ações).

## Prévia

Abra [`assets/preview/pipo-preview.html`](assets/preview/pipo-preview.html) no navegador para ver o Pipo animado e todos os estados: corpo grafite em quadrado arredondado e olhos geométricos, com cor usada só para comunicar estado.

## Planejamento

O plano completo está em [`docs/Pipo - Plano de Implementacao.docx`](docs/). Ele cobre escopo do MVP, stack, arquitetura, funcionalidades por fase, personalidade, privacidade, roadmap e riscos.

## Como rodar

Pré-requisitos no Windows: Node.js 22+, Rust (stable, via rustup), Visual Studio Build Tools com C++ e WebView2.

```bash
npm install
npm run tauri dev     # abre o Pipo em modo desenvolvimento
npm run tauri build   # gera os instaladores em src-tauri/target/release/bundle/
```

## Como usar

- **Clique no Pipo**: abre o menu radial (chat, área de transferência, sistema, gastos, configurações, ocultar). Quando você copia um erro ou um texto longo, aparece também um atalho para o Pipo explicar ou resumir.
- **Ctrl + Alt + Shift + Espaço**: abre e fecha o chat.
- **Arrastar**: move o Pipo, que para nas bordas da tela (e passa para outro monitor); a posição é lembrada.
- **Botão direito no Pipo** ou **ícone na bandeja**: chat, configurações, ocultar o Pipo e sair.
- **X** no canto do painel: fecha o painel.
- **Histórico** (no topo do chat): todas as conversas anteriores; clique para reabrir ou exclua as que não quiser. **+ Nova conversa** guarda a atual no histórico.

O Pipo reage ao que acontece: fica atento quando o painel abre, pensativo enquanto responde, satisfeito quando termina e em alerta quando algo dá errado. Também fica focado depois de alguns minutos programando, dorme com o computador parado e some sozinho quando um jogo ou vídeo está em tela cheia (modo jogo).

**Abas do painel:**

- **Chat**: conversa com a IA, com histórico.
- **Clipboard**: últimos textos copiados (só em memória; senhas e tokens são ignorados).
- **Sistema**: app em uso, humor do Pipo, CPU, memória, GPU NVIDIA e os processos que mais consomem.
- **Gastos**: custo estimado de hoje, do mês e de cada conversa, por modelo.
- **⚙ Configurações**: chave, modelo, sarcasmo, atalho, tamanho e cada sensor pode ser ligado ou desligado.

Sensores e alertas são lidos só no computador; nada disso é enviado para a internet.

**Chave da API:** na primeira execução, o Pipo procura `api/api-pipo.txt` (ou o arquivo indicado em `PIPO_API_KEY_FILE`) e guarda a chave no Gerenciador de Credenciais do Windows. Também dá para colar a chave ou importar o arquivo em **Configurações**. Depois de importada, o arquivo pode ser apagado.

**Configurações:** modelo (Haiku 5.5 por padrão, Sonnet 5.5 ou Opus 5.5), nível de sarcasmo, atalho global, tamanho do Pipo (Menor, Pequeno, Médio, Grande) e iniciar com o Windows.

## Estrutura

```
src/              interface (React + TypeScript)
  pipo/Pipo.tsx   personagem em SVG e seus estados
  pet/            janela do Pipo
  panel/          painel com chat e configurações
src-tauri/        backend (Rust) e configuração do app
  src/chat.rs     chat com a Claude API (streaming)
  src/usage.rs    tokens e custo de cada resposta
  src/sensors.rs  laço dos sensores: humor, modo jogo e alertas
  src/context.rs  app em primeiro plano, tela cheia e ociosidade
  src/hardware.rs CPU, memória, processos e GPU
  src/clipboard.rs histórico da área de transferência
  src/secrets.rs  chave no Gerenciador de Credenciais
  src/windows.rs  janelas, bandeja e atalhos
assets/           prévia visual e ícone
docs/             plano de implementação
```

## Stack

- **App desktop:** Tauri 2 (Rust)
- **Interface:** React + TypeScript + Vite
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
