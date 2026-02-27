# Dictea

[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8D8?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-backend-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=white)](https://react.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-frontend-3178C6?logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![macOS](https://img.shields.io/badge/macOS-10.15+-000000?logo=apple&logoColor=white)](https://www.apple.com/macos/)
[![License](https://img.shields.io/badge/License-MIT-green.svg)](package.json)

<img width="1784" height="1266" alt="image" src="https://github.com/user-attachments/assets/fa68b050-cca0-4f80-888d-ac34990c5a67" />

Intelligent voice dictation for macOS. Press a shortcut to start, speak, press again — your text is transcribed and pasted instantly.

## Features

- **Toggle dictation** — `Cmd+Shift+Space` to start recording, press again to transcribe and paste
- **Cancel anytime** — `Cmd+Shift+C` to cancel without pasting
- **3 STT engines** — OpenAI, Voxtral (Mistral), or Gemini (Google) — switch freely in settings
- **AI reformulation** — Clean up grammar, remove hesitations and repetitions
- **Auto-translation** — Translate to French, English, Spanish, German, Italian, or Portuguese
- **Floating overlay** — Minimal animated pill with real-time audio waveform
- **Native macOS look** — Dark glassmorphism theme, animated gradients, transparent title bar
- **Bilingual UI** — English and French

## STT Engines

| Engine | Transcription model | Reformulation model | API key |
|--------|--------------------|--------------------|---------|
| **OpenAI** | `gpt-4o-transcribe` | `gpt-4o-mini` | [platform.openai.com](https://platform.openai.com/api-keys) |
| **Voxtral** (Mistral) | `voxtral-mini-latest` | `mistral-small-latest` | [console.mistral.ai](https://console.mistral.ai/api-keys) |
| **Gemini** (Google) | `gemini-2.5-flash` | `gemini-2.5-flash-lite` | [aistudio.google.com](https://aistudio.google.com/apikey) |

## Installation

Download the latest release from [GitHub Releases](https://github.com/furybee/dictea/releases).

| Platform | File |
|----------|------|
| macOS | `.dmg` |
| Windows | `.msi` |
| Linux | `.deb`, `.AppImage` |

> **macOS:** The app is not signed with an Apple Developer certificate. After installing, run:
> ```bash
> xattr -cr /Applications/Dictea.app
> ```
> Then open the app normally.

## Development

### Prerequisites

- **macOS** 10.15+
- **Rust** — [rustup.rs](https://rustup.rs/)
- **Node.js** 18+ and **pnpm**

### Install & run

```bash
make install   # Install dependencies
make dev       # Development mode (hot reload)
```

### Build for production

```bash
make build
```

The `.dmg` and `.app` bundle will be in `src-tauri/target/release/bundle/`.

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd + Shift + Space` | Start / stop & paste |
| `Cmd + Shift + C` | Cancel (no paste) |

## Architecture

```
src-tauri/src/
├── lib.rs              # App state, Tauri commands, config, shortcuts
├── audio/              # Microphone capture (cpal, 48kHz → 16kHz)
├── stt/                # STT engines (OpenAI, Voxtral, Gemini)
└── pipeline/           # Real-time streaming pipeline

ui/
├── App.tsx             # Root component with i18n provider
├── components/
│   ├── SettingsView.tsx    # Settings layout + sidebar
│   ├── OverlayView.tsx     # Floating waveform pill
│   └── pages/              # Dictation, Engine, Shortcut, Settings
├── hooks/useConfig.ts      # Config loading & auto-save
├── i18n/                   # FR + EN translations
└── types.ts                # Shared types & constants
```

## Tech stack

| Component | Technology |
|-----------|-----------|
| Desktop framework | Tauri v2 |
| Backend | Rust |
| Frontend | React 19, TypeScript, Vite |
| Audio capture | cpal |
| HTTP client | reqwest |
| Icons | lucide-react |
| Fonts | Great Vibes, Nunito |

## Make commands

| Command | Description |
|---------|-------------|
| `make dev` | Dev mode with hot reload |
| `make build` | Production build |
| `make install` | Install dependencies |
| `make clean` | Clean build artifacts |
| `make kill` | Kill running processes |
| `make logs` | Tail application logs |
| `make release VERSION=x.y.z` | Tag & push a release |

Config is stored in `~/Library/Application Support/com.dictea.app/`.

## License

MIT
