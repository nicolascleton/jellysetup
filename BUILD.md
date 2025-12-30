# JellySetup - Build Guide

## Prerequisites

### All Platforms
- Node.js 18+ (https://nodejs.org/)
- Rust 1.70+ (https://rustup.rs/)
- pnpm or npm

### macOS
```bash
# Install Xcode Command Line Tools
xcode-select --install
```

### Windows
- Microsoft Visual Studio C++ Build Tools
- WebView2 (usually pre-installed on Windows 10/11)

### Linux (for development only)
```bash
sudo apt install libwebkit2gtk-4.0-dev build-essential curl wget libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

## Setup

1. Clone the repository:
```bash
git clone https://github.com/nicolascleton/jellysetup.git
cd jellysetup
```

2. Install dependencies:
```bash
npm install
```

3. Configure Supabase credentials:
```bash
cp .env.example .env
# Edit .env with your Supabase URL and anon key
```

## Development

Run in development mode:
```bash
npm run tauri:dev
```

## Build for Distribution

### macOS (.dmg)
```bash
npm run tauri:build
```
Output: `src-tauri/target/release/bundle/dmg/JellySetup_1.0.0_x64.dmg`

### Windows (.exe / .msi)
```bash
npm run tauri:build
```
Output: 
- `src-tauri/target/release/bundle/msi/JellySetup_1.0.0_x64_en-US.msi`
- `src-tauri/target/release/bundle/nsis/JellySetup_1.0.0_x64-setup.exe`

## Supabase Setup

1. Create a new Supabase project at https://supabase.com/
2. Go to SQL Editor and run the script from `supabase/schema.sql`
3. Copy your project URL and anon key from Settings > API
4. Add them to your `.env` file before building

## Project Structure

```
jellysetup/
├── src/                    # React frontend
│   ├── components/Wizard/  # Wizard step components
│   ├── lib/store.ts        # Zustand state management
│   └── App.tsx             # Main app component
├── src-tauri/              # Rust backend
│   └── src/
│       ├── main.rs         # Entry point + Tauri commands
│       ├── sd_card.rs      # SD card detection
│       ├── flash.rs        # Flash + configuration
│       ├── ssh.rs          # SSH client
│       ├── network.rs      # mDNS discovery
│       ├── supabase.rs     # Database client
│       └── crypto.rs       # SSH key generation
├── procedures/v1/          # Installation procedures
│   ├── steps.json          # Step definitions
│   └── templates/          # Jinja2 templates
└── supabase/
    └── schema.sql          # Database schema
```

## Signing & Notarization (macOS)

For distribution outside the App Store, you need to:

1. Get an Apple Developer account
2. Create a Developer ID certificate
3. Set environment variables:
```bash
export APPLE_CERTIFICATE="Developer ID Application: Your Name (TEAM_ID)"
export APPLE_CERTIFICATE_PASSWORD="your-cert-password"
export APPLE_ID="your@apple.id"
export APPLE_PASSWORD="app-specific-password"
export APPLE_TEAM_ID="YOUR_TEAM_ID"
```

## Troubleshooting

### "Unable to detect SD cards"
- Make sure you're running as administrator (Windows) or with sudo (macOS/Linux)
- Check that the SD card reader is properly connected

### "SSH connection failed"
- Verify the Pi is powered on and connected to the same network
- Check if the Pi completed its first boot (takes ~2 minutes)

### Build errors
```bash
# Clean and rebuild
cargo clean
rm -rf node_modules
npm install
npm run tauri:build
```
