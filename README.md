# Pipo

Bichinho virtual de desktop para Windows 11. Um personagem minimalista e tecnológico, intelectual e com um leve toque de sarcasmo, que flutua na tela e ajuda no dia a dia com atalhos, dicas e tarefas: desenvolvimento, documentos, jogos, lives e notícias.

> Projeto pessoal. Fases 1 (MVP), 2 (sensores) e 3 (ações) concluídas: Pipo flutuante, chat com a Claude API que age no computador (com confirmação), gastos por conversa, modo jogo, hardware, área de transferência, projetos, scripts, Git, portas, Docker, jogos, rotinas, notas, lembretes, pomodoro e OCR. Fase 4 (mundo externo) concluída: pesquisa na web com fontes, briefing diário e avisos de lives. Fase 5 (vida) concluída: comandos de voz com transcrição local (Whisper), respostas faladas, XP, níveis, evolução, acessórios, conquistas e olhos que seguem o mouse.

## Prévia

Abra [`assets/preview/pipo-preview.html`](assets/preview/pipo-preview.html) no navegador para ver o Pipo animado e todos os estados: corpo grafite em quadrado arredondado e olhos geométricos, com cor usada só para comunicar estado.

## Planejamento

O plano completo está em [`docs/Pipo - Plano de Implementacao.docx`](docs/). Ele cobre escopo do MVP, stack, arquitetura, funcionalidades por fase, personalidade, privacidade, roadmap e riscos.

## Como rodar

Pré-requisitos no Windows: Node.js 22+, Rust (stable, via rustup), Visual Studio Build Tools 2022 com C++ (inclui CMake e Clang/LLVM) e WebView2.

A voz usa o whisper.cpp, compilado com CMake e Clang. Se o `cmake` não estiver no PATH ou o Clang não for encontrado, aponte para os do Build Tools:

```powershell
$vs = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools"
$env:PATH += ";$vs\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"
$env:LIBCLANG_PATH = "$vs\VC\Tools\Llvm\x64\bin"
# Se o VS 2026 também estiver instalado, fixe o gerador que esse CMake conhece:
$env:CMAKE_GENERATOR = "Visual Studio 17 2022"
```

```bash
npm install
npm run tauri dev     # abre o Pipo em modo desenvolvimento
npm run tauri build   # gera os instaladores em src-tauri/target/release/bundle/
```

## Instalar e atualizar

O jeito certo de usar no dia a dia é instalado, como qualquer app: rode `src-tauri\target\release\bundle\nsis\Pipo_<versão>_x64-setup.exe`. Ele instala só para o seu usuário (sem pedir administrador) em `%LOCALAPPDATA%\Pipo` e cria atalhos no Menu Iniciar e na Área de Trabalho. Para atualizar, gere o instalador da versão nova e rode por cima (`Pipo_<versão>_x64-setup.exe /S` instala em silêncio). Desde a 0.3.0 o Pipo **se atualiza sozinho**: ele procura versões novas no GitHub Releases, avisa com um balão e atualiza em Configurações > Atualizações. Para publicar uma versão: suba o número da versão, faça o commit e rode `python scripts/release.py "o que mudou"` (precisa da chave de assinatura, que fica só neste computador). Seus dados ficam em `%APPDATA%\com.vitormouramaker.pipo`, separados do programa, então atualizar não apaga nada.

## Como usar

- **Clique no Pipo**: abre o menu radial (chat, área de transferência, sistema, gastos, configurações, ocultar). Quando você copia um erro ou um texto longo, aparece também um atalho para o Pipo explicar ou resumir.
- **Ctrl + Alt + Shift + Espaço**: abre e fecha o chat.
- **Ctrl + Alt + Shift + S**: recorte qualquer parte da tela e pergunte ao Pipo sobre ela (erro, gráfico, print de conversa); a imagem vai anexada ao chat.
- **Ctrl + Alt + Shift + V**: fala com o Pipo (também pelo 🎙 do chat ou pelo menu dele). Ele ouve até você fazer uma pausa, transcreve no próprio computador e responde em voz alta, com uma **voz natural** (Piper, voz neural que roda no PC, baixada uma vez em ⚙ → Voz). Para ele parar de falar: "Parar de falar" no chat, Esc, clique no Pipo ou o atalho de voz.
- **Ícone do Pipo no topo do painel**: nível, evolução, acessórios e conquistas.
- **Arrastar**: move o Pipo, que para nas bordas da tela (e passa para outro monitor); a posição é lembrada. Com o painel aberto, Pipo e painel andam juntos (arrastando qualquer um dos dois). Ao ser arrastado, o Pipo se inclina, estica com a velocidade e balança como gelatina quando você solta.
- **Modo ilha**: arraste o Pipo até encostar numa borda ou canto da tela; ele mostra que vai virar ilha e, ao soltar, vira (com um brilho rápido para ser notado). Puxe a ilha para longe da borda e ele volta a ser o Pipo. Também dá pelo menu do Pipo (ou pela bandeja): ele encolhe para uma pílula discreta grudada na borda da tela, como a ilha dinâmica do iPhone. Arraste para qualquer borda ou canto; ao passar o mouse ela expande e mostra a hora ou o pomodoro; clique para o chat; o botão dentro dela traz o Pipo de volta.
- **Botão direito no Pipo**: menu com a cara do Pipo, sempre ao lado dele (chat, falar, modo ilha, evolução, configurações, ocultar e sair). O **ícone na bandeja** tem as mesmas opções no menu do Windows.
- **Avisos**: lembretes, lives, pomodoro, níveis e alertas aparecem como balões do próprio Pipo ao lado dele, sem roubar o foco (nem de um jogo). Clique no balão para abrir a live ou a aba certa.
- **X** no canto do painel: fecha o painel. O botão ao lado expande o painel para a tela toda (e volta).
- **Sons**: efeitos curtos e suaves em cada interação (menus, arrasto, mensagens, avisos, nível novo). Volume e desligar em ⚙ → Aparência.
- **Histórico** (no topo do chat): todas as conversas anteriores; clique para reabrir ou exclua as que não quiser. **+ Nova conversa** guarda a atual no histórico.

O Pipo reage ao que acontece: fica atento quando o painel abre, pensativo enquanto responde, satisfeito quando termina e em alerta quando algo dá errado. Também fica focado depois de alguns minutos programando, dorme com o computador parado e some sozinho quando um jogo ou vídeo está em tela cheia no mesmo monitor que ele (modo jogo; janela maximizada não conta).

**Evolução:** o Pipo ganha XP quando você conversa, pede ações, conclui pomodoros, pesquisa, usa a voz e aparece todo dia. A cada nível ele fica mais completo: estágios Alpha, Beta (nível 5), Release (10) e LTS (20), com borda própria e acessórios liberados (antena, fone, gravata-borboleta e coroa), que podem ser guardados.

**Abas do painel:**

- **Chat**: conversa com a IA, com histórico. Pesquisa na web quando a pergunta depende de informação atual, com as fontes clicáveis.
- **Hoje**: briefing do dia (clima, notícias dos seus temas e lembretes; pode montar sozinho no horário), lives da Twitch, do YouTube e da Kick (é só colar o link; o Pipo descobre a plataforma) com aviso quando entram ao vivo, e pesquisa rápida. O Pipo usa ferramentas para agir (rodar scripts, ver portas, commit, lembretes…); tudo o que muda algo no computador pede **Permitir/Negar** antes.
- **Ações**:
  - **Dev**: projetos recentes do VS Code/Cursor (abrir, terminal, scripts no terminal ou em segundo plano com aviso ao terminar, mensagem de commit pela IA), tarefas em execução, portas em uso (com encerrar) e containers do Docker.
  - **Foco**: pomodoro, lembretes (inclusive os que se repetem: todo dia, dias úteis, toda semana ou todo mês), pausas para água e postura e notas rápidas.
  - **Atalhos**: perguntar sobre a tela, **organizar a pasta Downloads** (pastas por tipo, com desfazer, e o que dá para mandar para a Lixeira: instaladores antigos, downloads incompletos e cópias "(1)"); ler texto da tela (OCR do Windows, offline), jogos da Steam/Epic/Riot com favoritos, rotinas (abrir vários apps, links e projetos de uma vez) e arquivos recentes.
- **Arquivos**: arraste um PDF, Word, PowerPoint, imagem ou arquivo de texto/código em cima do Pipo (ou do painel) e ele vira anexo do chat, com atalhos para resumir, traduzir, ver os pontos principais ou explicar. No chat, peça "acha o contrato que baixei semana passada": o Pipo procura nas pastas Documentos, Downloads, Desktop e Imagens, lê o arquivo e responde.
- **Hoje** também mostra a **agenda** (Google Agenda pelo endereço secreto iCal, com aviso 10 min antes e botão "Entrar" em reuniões do Meet/Teams/Zoom), **o que está tocando** no Windows (com tocar/pausar e pular), **seu dia** (tempo em cada tipo de app, registrado só no computador) e o **GitHub** (seus PRs, revisões pedidas a você e a CI dos seus projetos, com aviso quando termina; vale para todas as contas logadas no `gh`, como trabalho e pessoal).
- **Pipo proativo** (nível equilibrado): no máximo 3 dicas por hora e nunca em jogo, foco ou com você longe: erro copiado, muitas mudanças sem commit, horas seguidas no PC, madrugada e o resumo do fim do dia. Ele também **lembra** do que você contar ("lembra que eu prefiro TypeScript"); a lista fica em ⚙ → Memória do Pipo.
- **Clipboard**: últimos textos copiados (só em memória; senhas e tokens são ignorados), com Resumir, Traduzir e Revisar.
- **Sistema**: app em uso, humor do Pipo, CPU, memória, GPU NVIDIA e os processos que mais consomem.
- **Gastos**: custo estimado de hoje, do mês e de cada conversa, por modelo.
- **⚙ Configurações**: chave, modelo, sarcasmo, atalho, tamanho e cada sensor pode ser ligado ou desligado.

Sensores e alertas são lidos só no computador; nada disso é enviado para a internet.

**Seus dados:** as configurações são salvas sozinhas assim que você muda algo. A cada início o Pipo guarda uma cópia de segurança do banco (as 15 mais recentes ficam em `%APPDATA%\com.vitormouramaker.pipo\backups`) e anota um resumo em `pipo.log` na mesma pasta.

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
  src/tools.rs    ferramentas que a IA pode usar no chat
  src/actions/    projetos, tarefas, Git, portas, Docker, jogos, rotinas, arquivos, notas, foco e OCR
  src/world/      lives (Twitch/YouTube/Kick) e briefing diário
  src/voice.rs    microfone e transcrição local (Whisper)
  src/life.rs     XP, níveis, evolução e conquistas
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
| 5 | Vida: voz, XP, evolução, acessórios (concluída) |
| 6 | Assistente completo: atualização automática, documentos, Pipo proativo, GitHub, agenda e música (concluída) |

## Privacidade

Os dados de contexto ficam no computador. O microfone só abre quando você pede, e o áudio é transcrito localmente; só o texto vai para o chat. A chave da API nunca entra no repositório: a pasta `api/` está no `.gitignore`, e o app guarda a chave no Gerenciador de Credenciais do Windows.
