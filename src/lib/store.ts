import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export interface Config {
  // Système Raspberry Pi
  hostname: string;
  systemUsername: string;
  systemPassword: string;

  // Réseau WiFi
  wifiSSID: string;
  wifiPassword: string;
  wifiCountry: string;

  // Locale
  timezone: string;
  keymap: string;

  // AllDebrid (obligatoire)
  alldebridKey: string;

  // Jellyfin
  jellyfinUsername: string;
  jellyfinPassword: string;
  jellyfinServerName: string;

  // Email admin (pour Jellyseerr)
  adminEmail?: string;

  // Optionnel
  yggPasskey?: string;
  discordWebhook?: string;
  cloudflareToken?: string;
}

export interface PiInfo {
  ip: string;
  hostname: string;
  macAddress?: string;
}

export interface SDCard {
  path: string;
  name: string;
  size: number;
  removable: boolean;
}

export interface SSHCredentials {
  publicKey: string;
  privateKey: string;
}

export interface JellyfinAuth {
  server_id: string;
  access_token: string;
  user_id: string;
}

interface Store {
  // Configuration utilisateur
  config: Config;
  setConfig: (config: Partial<Config>) => void;

  // Carte SD sélectionnée
  selectedSD: SDCard | null;
  setSelectedSD: (sd: SDCard | null) => void;

  // SSH credentials générées
  sshCredentials: SSHCredentials | null;
  setSSHCredentials: (creds: SSHCredentials | null) => void;

  // Info Pi détecté
  piInfo: PiInfo | null;
  setPiInfo: (info: PiInfo | null) => void;

  // ID installation Supabase
  installationId: string | null;
  setInstallationId: (id: string | null) => void;

  // Auth Jellyfin pour auto-login
  jellyfinAuth: JellyfinAuth | null;
  setJellyfinAuth: (auth: JellyfinAuth | null) => void;

  // Progression
  currentStep: string;
  setCurrentStep: (step: string) => void;
  progress: number;
  setProgress: (progress: number) => void;
  logs: string[];
  addLog: (log: string) => void;
  clearLogs: () => void;
}

export const useStore = create<Store>()(
  persist(
    (set) => ({
      // Config par défaut
      config: {
        // Système
        hostname: 'jellypi',
        systemUsername: 'maison',
        systemPassword: '',
        // WiFi
        wifiSSID: '',
        wifiPassword: '',
        wifiCountry: 'FR',
        // Locale
        timezone: 'Europe/Paris',
        keymap: 'fr',
        // Services
        alldebridKey: '',
        jellyfinUsername: '',
        jellyfinPassword: '',
        jellyfinServerName: 'jellypi',
        adminEmail: '',
      },
      setConfig: (newConfig) =>
        set((state) => ({
          config: { ...state.config, ...newConfig },
        })),

      // SD Card
      selectedSD: null,
      setSelectedSD: (sd) => set({ selectedSD: sd }),

      // SSH
      sshCredentials: null,
      setSSHCredentials: (creds) => set({ sshCredentials: creds }),

      // Pi Info
      piInfo: null,
      setPiInfo: (info) => set({ piInfo: info }),

      // Installation ID
      installationId: null,
      setInstallationId: (id) => set({ installationId: id }),

      // Auth Jellyfin
      jellyfinAuth: null,
      setJellyfinAuth: (auth) => set({ jellyfinAuth: auth }),

      // Progression
      currentStep: '',
      setCurrentStep: (step) => set({ currentStep: step }),
      progress: 0,
      setProgress: (progress) => set({ progress }),
      logs: [],
      addLog: (log) =>
        set((state) => ({
          logs: [...state.logs, `[${new Date().toLocaleTimeString()}] ${log}`],
        })),
      clearLogs: () => set({ logs: [] }),
    }),
    {
      name: 'jellysetup-storage-v5',
      partialize: (state) => ({
        // Ne persister que les données importantes
        config: state.config,
        selectedSD: state.selectedSD,
      }),
    }
  )
);
