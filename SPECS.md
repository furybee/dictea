# SPEC — Application STT locale temps réel

## 1. Objectif produit

Créer une application desktop cross-platform (macOS / Windows / Linux) qui permet :

- 🎤 De parler dans un micro
- ✍️ D'afficher le texte transcrit en temps réel
- 🌍 De forcer ou auto-détecter la langue
- 🔒 100% local, sans envoi réseau
- ⚡ Faible latence, utilisable en continu

---

## 2. Périmètre fonctionnel (V1)

### Fonctionnalités incluses

- Capture audio micro
- Transcription temps réel
- Détection automatique de langue ou langue forcée par l'utilisateur
- Copie du texte (clipboard)
- Historique local simple (session courante)

### Hors scope (V1)

- Traduction
- TTS
- Comptes utilisateurs
- Cloud / sync
- Enregistrement audio

---

## 3. UX / UI (V1)

### Écran principal

- 🎤 Bouton Start / Stop
- 📜 Zone texte live (scroll auto)
- 🌐 Sélecteur de langue :
  - Auto
  - FR, EN, ES, DE, etc.
- ⏱ Indicateur de latence (optionnel)
- 📋 Bouton "Copier"

### Comportement

- Texte apparaît progressivement
- Les mots peuvent être réécrits (partial → final)
- Stop = flush du buffer STT

---

## 4. Architecture générale

```
┌──────────────┐
│   UI (Tauri) │  React / Svelte
└──────┬───────┘
       │ IPC
┌──────▼────────┐
│   Core Rust   │
│               │
│  Audio Input  │  ← micro
│  STT Engine   │  ← Voxtral / Whisper.cpp
│  Pipeline     │
└──────┬────────┘
       │
┌──────▼────────┐
│   STT Model   │
│ (local files) │
└───────────────┘
```

---

## 5. Stack technique

| Composant | Technologie |
|-----------|-------------|
| **Langages** | Rust (core + STT), TypeScript (UI) |
| **Framework** | Tauri (desktop shell) |
| **STT** | Voxtral (Mistral), fallback : `whisper.cpp` |
| **Audio** | `cpal` (capture micro cross-platform), `ringbuf` (buffer temps réel) |

---

## 6. Pipeline STT temps réel

```
Micro
  ↓ (PCM 16kHz mono)
Audio buffer (ringbuf)
  ↓
Chunking (20–40 ms)
  ↓
STT inference
  ↓
Partial text
  ↓
Final text
  ↓
UI
```

### Contraintes

- Chunk court pour latence basse
- Traitement async (thread dédié)
- Backpressure gérée (drop frames si besoin)

---

## 7. Organisation du code (Rust)

```
src/
├── audio/
│   ├── mod.rs
│   └── microphone.rs      // capture PCM
├── stt/
│   ├── mod.rs
│   ├── engine.rs          // trait STTEngine
│   ├── voxtral.rs         // impl Voxtral
│   └── whisper.rs         // impl Whisper (fallback)
├── pipeline/
│   └── realtime.rs        // streaming logic
├── ipc/
│   └── commands.rs        // Tauri commands
└── main.rs
```

---

## 8. Interfaces clés

### STT Engine (trait)

```rust
pub trait SttEngine {
    fn load(model_path: &str) -> Result<Self>
    where
        Self: Sized;

    fn push_audio(&mut self, pcm: &[f32]);

    fn poll(&mut self) -> Option<SttEvent>;
}
```

### STT Event

```rust
pub enum SttEvent {
    Partial(String),
    Final(String),
}
```

---

## 9. IPC Tauri (exemples)

### Commands

- `start_listening(language: Option<String>)`
- `stop_listening()`
- `get_status()`

### Events UI

- `stt_partial`
- `stt_final`
- `stt_error`

---

## 10. Sécurité & Privacy

- Aucun appel réseau
- Modèles stockés localement
- Audio jamais persisté
- Permissions micro explicites

---

## 11. Packaging

### Binaries

- macOS (Intel + Apple Silicon)
- Windows (`.msi`)
- Linux (AppImage / `.deb`)

### Taille cible

- Core app < 10 MB
- Modèle STT séparé (download optionnel)

---

## 12. Performances attendues (Voxtral)

| Machine | Latence |
|---------|---------|
| M1 / M2 | ~100–200 ms |
| Intel i7 | ~200–300 ms |
| Laptop low-end | ~400 ms |

---

## 13. Tests

### Tests unitaires

- Audio chunking
- Pipeline backpressure
- STT mock

### Tests manuels

- Accent fort
- Parole continue
- Silence long
- Micro débranché

---

## 14. Évolutions futures (V2+)

- Traduction live
- TTS (lecture texte)
- Raccourci clavier global
- Mode dictée OS
- Export texte
- Mode "push-to-talk"

---

## Conclusion

Cette spec permet de :

- ✅ Shipper vite
- ✅ Rester 100% local
- ✅ Avoir une base propre et scalable
- ✅ Accueillir STT / traduction / TTS plus tard
