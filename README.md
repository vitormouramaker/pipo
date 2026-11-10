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
- **Ctrl + Alt + Shift + S**: recorte qualquer parte da tela e pergunte ao Pipo sobre ela (erro, gráfico, print de conversa); o print aparece em miniatura no chat (clique para ver inteiro).
- **Ctrl + Alt + Shift + L**: legenda ao vivo do som do PC (live, vídeo, call) numa faixa por cima de tudo, traduzida para o português; o áudio é transcrito no próprio computador. Também pelo botão CC do mini-player.
- **Ctrl + Alt + Shift + P**: perdeu o Pipo de vista? Ele pulsa em ciano na tela (e aparece, se estava escondido). Também pela bandeja: "Encontrar o Pipo".
- **Ctrl + Alt + Shift + V**: fala com o Pipo (também pelo 🎙 do chat ou pelo menu dele). Ele ouve até você fazer uma pausa, transcreve no próprio computador e responde em voz alta, com uma **voz natural** (Piper, voz neural que roda no PC, baixada uma vez em ⚙ → Voz). Para ele parar de falar: "Parar de falar" no chat, Esc, clique no Pipo ou o atalho de voz.
- **"Ei, Pipo"** (⚙ → Voz): chame o Pipo pela voz, sem atalho. "Ei, Pipo" abre a escuta e "Ei, Pipo, abre o Spotify" já faz o pedido. Tudo é ouvido e entendido no próprio PC (nada é gravado nem enviado), só nos modos que você escolher. No **modo conversa**, o Pipo responde falando e volta a ouvir, até você ficar em silêncio ou dizer "tchau".
- **Ícone do Pipo no topo do painel**: nível, evolução, acessórios e conquistas.
- **Arrastar**: move o Pipo, que para nas bordas da tela (e passa para outro monitor); a posição é lembrada. Com o painel aberto, Pipo e painel andam juntos (arrastando qualquer um dos dois). Ao ser arrastado, o Pipo se inclina, estica com a velocidade e balança como gelatina quando você solta.
- **Modo ilha**: arraste o Pipo até encostar numa borda ou canto da tela; ele mostra que vai virar ilha e, ao soltar, vira (com um brilho rápido para ser notado). Puxe a ilha para longe da borda e ele volta a ser o Pipo. Também dá pelo menu do Pipo (ou pela bandeja): ele encolhe para uma pílula discreta grudada na borda da tela, como a ilha dinâmica do iPhone. Arraste para qualquer borda ou canto; ao passar o mouse ela expande e mostra a hora ou o pomodoro; clique para o chat; o botão dentro dela traz o Pipo de volta.
- **Botão direito no Pipo**: menu com a cara do Pipo, sempre ao lado dele (chat, falar, modo ilha, evolução, configurações, ocultar e sair). O **ícone na bandeja** tem as mesmas opções no menu do Windows.
- **Avisos**: lembretes, lives, pomodoro, níveis e alertas aparecem como balões do próprio Pipo ao lado dele, sem roubar o foco (nem de um jogo). Clique no balão para abrir a aba certa; no aviso de live, abre o **mini-player** (janela pequena por cima de tudo, que dá para arrastar e redimensionar).
- **Modos** (⚙ → Modos): normal, trabalho, estudo e jogo. Em "O que funciona em cada modo" você liga ou desliga, aba por aba, o que continua chegando em cada um (lembretes, lives, GitHub, e-mail, alertas, LoL, o Pipo puxar assunto…). Os modos ligam sozinhos com um app aberto ou num horário, ou na mão. Por padrão, abrir o LoL ou o Valorant liga o modo jogo.
- **Atalhos de texto** (Ações → Atalhos): digite um gatilho como `;email` em qualquer app e ele vira o texto salvo (aceita `{data}` e `{hora}`). Só as últimas letras digitadas ficam na memória para reconhecer o gatilho; nada é gravado.
- **X** no canto do painel: fecha o painel. O botão ao lado expande o painel para a tela toda (e volta).
- **Sons**: efeitos curtos e suaves em cada interação (menus, arrasto, mensagens, avisos, nível novo). Volume e desligar em ⚙ → Aparência.
- **Histórico** (no topo do chat): todas as conversas anteriores; clique para reabrir ou exclua as que não quiser. **+ Nova conversa** guarda a atual no histórico.

O Pipo reage ao que acontece: fica atento quando o painel abre, pensativo enquanto responde, satisfeito quando termina e em alerta quando algo dá errado. Também fica focado depois de alguns minutos programando, dorme com o computador parado e some sozinho quando um jogo ou vídeo está em tela cheia no mesmo monitor que ele (modo jogo; janela maximizada não conta).

**Visual e jeito:** na aba do Pipo, em Cores, escolha a cor dele (Grafite, Neve, Meia-noite e, com o nível, Menta, Coral e Ouro) e da ilha (cor do Pipo, grafite ou vidro). Ele dança no ritmo da música que estiver tocando e comemora vitórias no LoL. Em ⚙ → Inteligência dá para escolher o jeito de falar (equilibrado, técnico, engraçado, direto ou professor) e o tamanho das respostas; ele também lembra dos assuntos das últimas conversas.

**Evolução:** o Pipo ganha XP quando você conversa, pede ações, conclui pomodoros, pesquisa, usa a voz e aparece todo dia. A cada nível ele fica mais completo: estágios Alpha, Beta (nível 5), Release (10) e LTS (20), com borda própria e acessórios liberados (antena, fone, óculos escuros, gravata-borboleta, cachecol e coroa), que podem ser guardados.

**Abas do painel:**

- **Chat**: conversa com a IA, com histórico. Pesquisa na web quando a pergunta depende de informação atual, com as fontes clicáveis.
- **Hoje**: briefing do dia (clima, notícias dos seus temas e lembretes; pode montar sozinho no horário), lives da Twitch, do YouTube e da Kick (é só colar o link; o Pipo descobre a plataforma) com aviso quando entram ao vivo, e pesquisa rápida. O Pipo usa ferramentas para agir (rodar scripts, ver portas, commit, lembretes…); tudo o que muda algo no computador pede **Permitir/Negar** antes.
- **Ações**:
  - **Dev**: projetos recentes do VS Code/Cursor (abrir, terminal, scripts no terminal ou em segundo plano com aviso ao terminar, mensagem de commit pela IA), tarefas em execução, portas em uso (com encerrar) e containers do Docker.
  - **Foco**: pomodoro, lembretes (inclusive os que se repetem: todo dia, dias úteis, toda semana ou todo mês), pausas para água e postura e notas rápidas.
  - **Atalhos**: perguntar sobre a tela, **organizar a pasta Downloads** (pastas por tipo, com desfazer, e o que dá para mandar para a Lixeira: instaladores antigos, downloads incompletos, arquivos repetidos pelo conteúdo — mesmo em subpastas — e compactados já extraídos; e, se quiser, organiza sozinho uma vez por dia); ler texto da tela (OCR do Windows, offline), jogos da Steam/Epic/Riot com favoritos, rotinas (abrir vários apps, links e projetos de uma vez) e arquivos recentes.
- **Arquivos**: arraste um PDF, Word, PowerPoint, imagem ou arquivo de texto/código em cima do Pipo (ou do painel) e ele vira anexo do chat, com atalhos para resumir, traduzir, ver os pontos principais ou explicar. No chat, peça "acha o contrato que baixei semana passada": o Pipo procura nas pastas Documentos, Downloads, Desktop e Imagens, lê o arquivo e responde.
- **Resumir** (Ações → Resumir, ou no chat: "resume esse vídeo …"): vídeo do YouTube, página da web, PDF ou Word grande. Textos longos são resumidos em partes. No YouTube, o Pipo usa a legenda quando o YouTube libera; senão, resume pela descrição e explica como colar a transcrição.
- **Texto selecionado em qualquer app** (Ctrl+Alt+Shift+E): selecione um texto no Word, no WhatsApp, no navegador… e aperte o atalho. Ao lado do mouse aparecem Corrigir, Traduzir, Resumir, Mais formal, Mais curto e Simpático (ou escreva o que quer, como "em espanhol"); "Substituir" cola o resultado no lugar da seleção, e o que você tinha copiado continua lá.
- **Guardião do PC** (⚙ → Sensores): avisa quando um disco está quase cheio (dizendo quanto há na Lixeira e em Downloads), quando o app em uso trava (um clique e o Pipo fecha) e cuida do jogo: lembra antes da partida se o Windows está esperando reiniciar, conta depois se o Windows Update ficou trabalhando e confere o Modo de Jogo do Windows, que segura as atualizações.
- **Diário** (aba Diário): o Pipo anota sozinho o seu dia no PC — quanto tempo em cada app e projeto (com os arquivos e páginas), avisos, downloads, hábitos cumpridos, conversas, notas e partidas — numa linha do tempo com total por tipo de app, busca nos últimos 90 dias ("quando baixei o contrato?") e o botão "Resumir o dia". Também pelo chat ("o que eu fiz ontem à tarde?") e pelo Telegram (/diario). Fica só no PC; janelas anônimas e cofres de senha ficam de fora.
- **Hábitos e metas** (aba Hoje): hábitos de feito/não feito ou com quantidade ("8 copos de água"), nos dias que você escolher e com lembrete no horário (com o botão **Feito** no aviso). O Pipo conta a sequência de dias (folga não quebra), avisa às 21h se ela estiver para quebrar, ganha XP quando você cumpre e mostra os hábitos do dia na ilha. Pelo chat ou Telegram: "cria o hábito ler 20 páginas", "bebi 2 copos de água".
- **Hoje** também mostra a **agenda** (Google Agenda conectada pelo login do Google — o Pipo cria, muda e apaga eventos pelo chat ou pelo Telegram, sempre com confirmação, e avisa a hora de sair quando o evento tem endereço — e outras agendas pelo endereço secreto iCal; aviso antes de cada compromisso e botão "Entrar" em reuniões do Meet/Teams/Zoom), **o que está tocando** no Windows (com tocar/pausar e pular), **seu dia** (tempo em cada tipo de app, registrado só no computador) e o **GitHub** (seus PRs, revisões pedidas a você e a CI dos seus projetos, com aviso quando termina; vale para todas as contas logadas no `gh`, como trabalho e pessoal).
- **League of Legends**: com o cliente aberto, ao travar o campeão chega uma dica da Claude com a build e as runas do patch atual (pesquisadas na web), mostrada na aba Hoje com as imagens oficiais de runas, feitiços e itens, o retrato do seu campeão e dos adversários para a sua rota e os adversários; a aba Hoje mostra as últimas partidas (taxa de vitória, KDA, farm, mais jogado) com "Analisar minha evolução", e depois de 3 derrotas seguidas o Pipo sugere uma pausa; na partida, aviso 1 minuto antes de Dragão e Barão voltarem; no fim, resultado, KDA e farm por minuto, com atalho para pedir uma análise. Usa só as APIs locais que o próprio LoL abre no PC.
- **Celular (Telegram)**: crie um bot no @BotFather, cole o token em ⚙ → Integrações e mande uma mensagem para ele: os avisos escolhidos (lembretes, lives, GitHub, e-mail…) chegam no Telegram, se quiser só quando você estiver longe do PC. E dá para **conversar com o Pipo pelo celular** como no chat ("me lembra às 18h de…", "o build terminou?"); ações no PC pedem Permitir/Negar por botão. Comandos sem custo: /status, /lembretes, /apagar, /email, /semana, /novo e /ajuda. Só a sua conversa é atendida. E dá para **controlar o PC pelo celular**: /print manda um print da tela, /arquivo contrato procura e manda um arquivo (você escolhe na lista) e /bloquear bloqueia o PC; o Pipo avisa no PC quando um print ou arquivo sai.
- **Automações** (Ações → Automações): "quando X, faça Y". Gatilhos: programa abriu/fechou, horário, modo ligou/desligou, voltou ao PC, Pipo abriu. Ações em sequência: abrir programa, pasta ou link (inclusive `spotify:`), rodar rotina, avisar, mandar no Telegram, trocar modo, pomodoro, música, modo ilha e esperar. Tem modelos prontos e "Testar agora".
- **Relatório semanal** (domingo 19h, ajustável): tempo no PC por tipo de app comparado à semana anterior, pomodoros, commits, partidas de LoL, XP e gastos, com um comentário do Pipo, na aba Hoje e no Telegram.
- **Gmail**: com uma senha de app, a aba Hoje mostra os não lidos da caixa principal e o botão "Resumir com o Pipo"; clique num e-mail para ler inteiro e **responder com o Pipo**: ele escreve um rascunho, você edita e envia (com confirmação) ou guarda nos rascunhos do Gmail. No chat: "tenho e-mail importante?", "responde o e-mail da Ana dizendo que topo". Nada é marcado como lido.
- **Pipo proativo** (nível equilibrado): no máximo 3 dicas por hora e nunca em jogo, foco ou com você longe: erro copiado, muitas mudanças sem commit, horas seguidas no PC, madrugada e o resumo do fim do dia. Ele também **lembra** do que você contar ("lembra que eu prefiro TypeScript"); a lista fica em ⚙ → Memória do Pipo.
- **Clipboard**: últimos textos e imagens copiados (só em memória; senhas e tokens são ignorados), com busca, filtros, itens fixados (os textos fixados ficam guardados), Resumir, Traduzir, Revisar e Perguntar ao Pipo sobre a imagem. **Ctrl + Alt + Shift + T** cola o que está copiado como texto puro.
- **Buscar nos meus arquivos** (Ações → Atalhos, ou no chat "acha o PDF do contrato"): procura pelo assunto dentro de PDFs, Word, PowerPoint e textos de Documentos, Downloads, Área de Trabalho e Imagens. O índice fica só no PC.
- **Comandos de barra no chat**: digite `/` para ver /novo, /tela, /lembrete, /lembretes, /email, /semana, /pomodoro, /ilha, /encontrar e /ajuda.
- **Sistema**: app em uso, humor do Pipo, CPU, memória, GPU NVIDIA e os processos que mais consomem.
- **Gastos**: custo estimado de hoje, do mês e de cada conversa, por modelo.
- **⚙ Configurações**: chave, modelo, sarcasmo, atalho, tamanho e cada sensor pode ser ligado ou desligado.

Sensores e alertas são lidos só no computador; nada disso é enviado para a internet.

**Instalação limpa:** ao abrir, o Pipo garante que o "iniciar com o Windows" e os atalhos apontam para ele mesmo e avisa se achar uma cópia antiga (que pode ser mandada para a Lixeira em ⚙ → Atualizações). As configurações têm uma cópia completa: se uma versão antiga abrir por engano e apagar campos novos, eles voltam.

**Backup na nuvem e vários PCs** (⚙ → Backup na nuvem): no **Google Drive pela internet** (sem instalar nada; um registro único de ~5 min no Google Cloud, com passo a passo na tela, e login pelo navegador) ou numa pasta sincronizada (Google Drive para computador ou OneDrive). O Pipo guarda uma cópia por dia (ou na hora); em outro PC, conecte a mesma conta (ou pasta) e restaure. Chaves e senhas não vão para a nuvem.

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
  src/world/      lives, briefing, GitHub, agenda, música, LoL, Telegram e Gmail
  src/modes.rs    modos trabalho, estudo e jogo
  src/miniplayer.rs mini-player de lives
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
| 7 | Mais completo: perguntar sobre a tela, Downloads, mini-player, modos, atalhos de texto, LoL, Telegram e Gmail (concluída) |
| 8 | Ainda mais: Pipo pelo celular, relatório semanal, automações, busca inteligente, LoL avançado, legenda ao vivo, visual e backup na nuvem (concluída) |

## Privacidade

Os dados de contexto ficam no computador. O diário fica só no computador, por 90 dias, e pode ser desligado ou apagado na própria aba. O microfone só abre quando você pede (ou fica atento ao "Ei, Pipo", se você ligar), e o áudio é transcrito localmente; só o texto vai para o chat. O token do bot do Telegram e a senha de app do Gmail ficam no Gerenciador de Credenciais; e-mails só vão para a Claude quando você pede um resumo. A chave da API nunca entra no repositório: a pasta `api/` está no `.gitignore`, e o app guarda a chave no Gerenciador de Credenciais do Windows.
