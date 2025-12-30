# JellySetup

Application desktop cross-platform (Mac/Windows) pour configurer automatiquement un Raspberry Pi Media Center.

## Features

- Flash automatique de carte SD avec Raspberry Pi OS
- Configuration WiFi et SSH pré-injectée
- Interface wizard simple pour utilisateurs non-techniques
- Découverte automatique du Pi sur le réseau
- Installation complète du media stack via SSH
- Sauvegarde des credentials dans Supabase

## Stack Technique

- **Frontend**: React + TypeScript + TailwindCSS
- **Backend**: Rust (Tauri)
- **Base de données**: Supabase
- **Distribution**: DMG (Mac) / EXE (Windows)

## Services installés

| Service | Port | Description |
|---------|------|-------------|
| Jellyfin | 8096 | Serveur multimédia |
| Jellyseerr | 5055 | Demandes de contenu |
| Radarr | 7878 | Gestion films |
| Sonarr | 8989 | Gestion séries |
| Prowlarr | 9696 | Indexeurs |
| Bazarr | 6767 | Sous-titres |
| Decypharr | 8282 | AllDebrid + WebDAV |
| FlareSolverr | 8191 | Bypass Cloudflare |

## Développement

### Prérequis

- Node.js 18+
- Rust 1.70+
- [Tauri CLI](https://tauri.app/v1/guides/getting-started/prerequisites)

### Installation

```bash
# Installer les dépendances
npm install

# Lancer en mode développement
npm run tauri:dev
```

### Build

```bash
# Build pour la plateforme actuelle
npm run tauri:build
```

## Structure du projet

```
jellysetup/
├── src-tauri/          # Backend Rust
│   ├── src/
│   │   ├── main.rs     # Point d'entrée + commandes Tauri
│   │   ├── sd_card.rs  # Détection cartes SD
│   │   ├── flash.rs    # Flash SD + config
│   │   ├── ssh.rs      # Client SSH
│   │   ├── network.rs  # Découverte réseau
│   │   ├── supabase.rs # Client Supabase
│   │   └── crypto.rs   # Génération clés SSH
│   └── Cargo.toml
├── src/                # Frontend React
│   ├── components/
│   │   └── Wizard/     # Composants du wizard
│   ├── lib/
│   │   └── store.ts    # État global (Zustand)
│   └── App.tsx
├── procedures/         # Procédures d'installation
│   └── v1/
│       └── steps.json  # Étapes de configuration
└── package.json
```

## Configuration Supabase

Créer les tables avec le script SQL fourni dans `ARCHITECTURE.md`.

## Licence

MIT
