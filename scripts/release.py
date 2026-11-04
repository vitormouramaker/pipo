"""
Publica uma versão do Pipo para a atualização automática.

1. Gera o instalador assinado (a chave privada fica fora do repositório, em
   %USERPROFILE%\\.tauri\\pipo-updater.key; sem ela não dá para publicar).
2. Monta o latest.json que o Pipo instalado consulta.
3. Cria o release vX.Y.Z no GitHub com o instalador e o latest.json.

Uso:  python scripts/release.py "O que mudou nesta versão"  [--install]
      --install também instala a versão neste computador (sem esperar o aviso).
Antes: suba a versão em src-tauri/tauri.conf.json, src-tauri/Cargo.toml e package.json,
faça o commit e o push.
"""
import datetime
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPO = "vitormouramaker/pipo"
KEY = Path(os.environ["USERPROFILE"]) / ".tauri" / "pipo-updater.key"
VS = Path(r"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools")


def run(cmd, **kw):
    print("$", " ".join(str(c) for c in cmd), flush=True)
    return subprocess.run(cmd, check=True, **kw)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    notes = args[0] if args else "Melhorias e correções."
    version = json.loads((ROOT / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))["version"]
    if not KEY.exists():
        sys.exit(f"Chave de assinatura não encontrada em {KEY}.")

    env = dict(os.environ)
    env["TAURI_SIGNING_PRIVATE_KEY"] = KEY.read_text(encoding="utf-8")
    env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = ""
    # whisper.cpp (voz) precisa do CMake e do Clang do Visual Studio Build Tools.
    env.setdefault("CMAKE_GENERATOR", "Visual Studio 17 2022")
    env.setdefault("LIBCLANG_PATH", str(VS / "VC" / "Tools" / "Llvm" / "x64" / "bin"))
    env["PATH"] = env["PATH"] + os.pathsep + str(VS / "Common7" / "IDE" / "CommonExtensions" / "Microsoft" / "CMake" / "CMake" / "bin")
    run(["npm", "run", "tauri", "build"], cwd=ROOT, env=env, shell=True)

    nsis = ROOT / "src-tauri" / "target" / "release" / "bundle" / "nsis"
    setup = nsis / f"Pipo_{version}_x64-setup.exe"
    sig = setup.with_name(setup.name + ".sig")
    if not (setup.exists() and sig.exists()):
        sys.exit("Instalador ou assinatura não encontrados; o build falhou?")

    latest = {
        "version": version,
        "notes": notes,
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {
            "windows-x86_64": {
                "signature": sig.read_text(encoding="utf-8").strip(),
                "url": f"https://github.com/{REPO}/releases/download/v{version}/{setup.name}",
            }
        },
    }
    latest_path = nsis / "latest.json"
    latest_path.write_text(json.dumps(latest, ensure_ascii=False, indent=2), encoding="utf-8")

    gh_env = dict(os.environ)
    gh_env["GH_TOKEN"] = subprocess.run(
        ["gh", "auth", "token", "--user", "vitormouramaker"], check=True, capture_output=True, text=True
    ).stdout.strip()
    run(
        ["gh", "release", "create", f"v{version}", str(setup), str(latest_path),
         "--repo", REPO, "--title", f"Pipo {version}", "--notes", notes],
        env=gh_env,
    )

    if "--install" in sys.argv:
        run([str(setup), "/S"])
        print("Instalado. Abra o Pipo pelo Menu Iniciar.")


if __name__ == "__main__":
    main()
