import { useState } from 'react';
import { ArrowLeft, ArrowRight, Eye, EyeOff, ExternalLink, Wifi, Key, User, Info, Monitor } from 'lucide-react';
import { open } from '@tauri-apps/api/shell';
import { Config } from '../../lib/store';

interface ConfigFormProps {
  config: Config;
  onConfigChange: (config: Partial<Config>) => void;
  onNext: () => void;
  onBack: () => void;
}

const WIFI_COUNTRIES = [
  { code: 'FR', name: 'France' },
  { code: 'BE', name: 'Belgique' },
  { code: 'CH', name: 'Suisse' },
  { code: 'CA', name: 'Canada' },
  { code: 'US', name: 'USA' },
  { code: 'GB', name: 'UK' },
  { code: 'DE', name: 'Allemagne' },
  { code: 'ES', name: 'Espagne' },
  { code: 'IT', name: 'Italie' },
];

const TIMEZONES = [
  { value: 'Europe/Paris', name: 'Paris' },
  { value: 'Europe/Brussels', name: 'Bruxelles' },
  { value: 'Europe/Zurich', name: 'Zurich' },
  { value: 'America/Montreal', name: 'Montréal' },
  { value: 'America/New_York', name: 'New York' },
  { value: 'Europe/London', name: 'Londres' },
];

export default function ConfigForm({ config, onConfigChange, onNext, onBack }: ConfigFormProps) {
  const [showSystemPassword, setShowSystemPassword] = useState(false);
  const [showWifiPassword, setShowWifiPassword] = useState(false);
  const [showJellyfinPassword, setShowJellyfinPassword] = useState(false);
  const [errors, setErrors] = useState<Record<string, string>>({});

  const validate = (): boolean => {
    const newErrors: Record<string, string> = {};
    // Système
    if (!config.hostname.trim()) newErrors.hostname = 'Requis';
    if (!config.systemUsername.trim()) newErrors.systemUsername = 'Requis';
    if (!config.systemPassword.trim()) newErrors.systemPassword = 'Requis';
    else if (config.systemPassword.length < 4) newErrors.systemPassword = 'Min 4 caractères';
    // WiFi
    if (!config.wifiSSID.trim()) newErrors.wifiSSID = 'Requis';
    if (!config.wifiPassword.trim()) newErrors.wifiPassword = 'Requis';
    // Services
    if (!config.alldebridKey.trim()) newErrors.alldebridKey = 'Requis';
    if (!config.jellyfinUsername.trim()) newErrors.jellyfinUsername = 'Requis';
    if (!config.jellyfinPassword.trim()) newErrors.jellyfinPassword = 'Requis';
    else if (config.jellyfinPassword.length < 4) newErrors.jellyfinPassword = 'Min 4 caractères';
    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  const handleNext = () => {
    if (validate()) onNext();
  };

  return (
    <div className="space-y-4">
      {/* Système Raspberry Pi */}
      <div className="card !p-4">
        <div className="flex items-center gap-2 mb-3">
          <Monitor className="w-4 h-4 text-green-400" />
          <span className="font-medium text-white text-sm">Système Raspberry Pi</span>
        </div>
        <p className="text-xs text-zinc-500 mb-3 flex items-center gap-1">
          <Info className="w-3 h-3" />
          Créez vos identifiants pour accéder au Pi en SSH
        </p>
        <div className="grid grid-cols-3 gap-3">
          <div>
            <input
              type="text"
              value={config.hostname}
              onChange={(e) => onConfigChange({ hostname: e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, '') })}
              className={`input-field text-sm py-2.5 ${errors.hostname ? 'input-field-error' : ''}`}
              placeholder="Hostname"
            />
          </div>
          <div>
            <input
              type="text"
              value={config.systemUsername}
              onChange={(e) => onConfigChange({ systemUsername: e.target.value.toLowerCase().replace(/[^a-z0-9_]/g, '') })}
              className={`input-field text-sm py-2.5 ${errors.systemUsername ? 'input-field-error' : ''}`}
              placeholder="Utilisateur"
            />
          </div>
          <div className="relative">
            <input
              type={showSystemPassword ? 'text' : 'password'}
              value={config.systemPassword}
              onChange={(e) => onConfigChange({ systemPassword: e.target.value })}
              className={`input-field text-sm py-2.5 pr-10 ${errors.systemPassword ? 'input-field-error' : ''}`}
              placeholder="Mot de passe"
            />
            <button
              type="button"
              onClick={() => setShowSystemPassword(!showSystemPassword)}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-white"
            >
              {showSystemPassword ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
            </button>
          </div>
        </div>
        <div className="grid grid-cols-2 gap-3 mt-3">
          <select
            value={config.wifiCountry}
            onChange={(e) => onConfigChange({ wifiCountry: e.target.value })}
            className="input-field text-sm py-2.5"
          >
            {WIFI_COUNTRIES.map((c) => (
              <option key={c.code} value={c.code}>{c.name}</option>
            ))}
          </select>
          <select
            value={config.timezone}
            onChange={(e) => onConfigChange({ timezone: e.target.value })}
            className="input-field text-sm py-2.5"
          >
            {TIMEZONES.map((tz) => (
              <option key={tz.value} value={tz.value}>{tz.name}</option>
            ))}
          </select>
        </div>
      </div>

      {/* WiFi */}
      <div className="card !p-4">
        <div className="flex items-center gap-2 mb-3">
          <Wifi className="w-4 h-4 text-blue-400" />
          <span className="font-medium text-white text-sm">WiFi</span>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <input
              type="text"
              value={config.wifiSSID}
              onChange={(e) => onConfigChange({ wifiSSID: e.target.value })}
              className={`input-field text-sm py-2.5 ${errors.wifiSSID ? 'input-field-error' : ''}`}
              placeholder="Nom du réseau"
            />
          </div>
          <div className="relative">
            <input
              type={showWifiPassword ? 'text' : 'password'}
              value={config.wifiPassword}
              onChange={(e) => onConfigChange({ wifiPassword: e.target.value })}
              className={`input-field text-sm py-2.5 pr-10 ${errors.wifiPassword ? 'input-field-error' : ''}`}
              placeholder="Mot de passe"
            />
            <button
              type="button"
              onClick={() => setShowWifiPassword(!showWifiPassword)}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-white"
            >
              {showWifiPassword ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
            </button>
          </div>
        </div>
      </div>

      {/* AllDebrid */}
      <div className="card !p-4">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <Key className="w-4 h-4 text-red-400" />
            <span className="font-medium text-white text-sm">AllDebrid API</span>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => open('https://alldebrid.com/register/')}
              className="text-xs text-zinc-400 hover:text-white flex items-center gap-1"
            >
              Créer un compte <ExternalLink className="w-3 h-3" />
            </button>
            <button
              onClick={() => open('https://alldebrid.com/apikeys/')}
              className="text-xs text-purple-400 hover:text-purple-300 flex items-center gap-1"
            >
              Obtenir la clé <ExternalLink className="w-3 h-3" />
            </button>
          </div>
        </div>
        <input
          type="text"
          value={config.alldebridKey}
          onChange={(e) => onConfigChange({ alldebridKey: e.target.value })}
          className={`input-field text-sm py-2.5 font-mono ${errors.alldebridKey ? 'input-field-error' : ''}`}
          placeholder="Votre clé API"
        />
      </div>

      {/* YGG Passkey */}
      <div className="card !p-4">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <Key className="w-4 h-4 text-yellow-400" />
            <span className="font-medium text-white text-sm">YGG Passkey</span>
            <span className="text-xs text-zinc-500">(optionnel)</span>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => open('https://www.yggtorrent.top/user/register')}
              className="text-xs text-zinc-400 hover:text-white flex items-center gap-1"
            >
              Créer un compte <ExternalLink className="w-3 h-3" />
            </button>
            <button
              onClick={() => open('https://yggapi.eu/')}
              className="text-xs text-yellow-400 hover:text-yellow-300 flex items-center gap-1"
            >
              Obtenir la passkey <ExternalLink className="w-3 h-3" />
            </button>
          </div>
        </div>
        <p className="text-xs text-zinc-500 mb-2 flex items-center gap-1">
          <Info className="w-3 h-3" />
          Connectez-vous sur YGG API et copiez votre passkey depuis votre compte
        </p>
        <input
          type="text"
          value={config.yggPasskey || ''}
          onChange={(e) => onConfigChange({ yggPasskey: e.target.value })}
          className="input-field text-sm py-2.5 font-mono"
          placeholder="Votre passkey YGG (32 caractères)"
        />
      </div>

      {/* Jellyfin */}
      <div className="card !p-4">
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-2">
            <User className="w-4 h-4 text-purple-400" />
            <span className="font-medium text-white text-sm">Compte Jellyfin</span>
          </div>
        </div>
        <p className="text-xs text-zinc-500 mb-3 flex items-center gap-1">
          <Info className="w-3 h-3" />
          Choisissez vos identifiants — le compte sera créé automatiquement
        </p>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <input
              type="text"
              value={config.jellyfinUsername}
              onChange={(e) => onConfigChange({ jellyfinUsername: e.target.value })}
              className={`input-field text-sm py-2.5 ${errors.jellyfinUsername ? 'input-field-error' : ''}`}
              placeholder="Nom d'utilisateur"
            />
          </div>
          <div className="relative">
            <input
              type={showJellyfinPassword ? 'text' : 'password'}
              value={config.jellyfinPassword}
              onChange={(e) => onConfigChange({ jellyfinPassword: e.target.value })}
              className={`input-field text-sm py-2.5 pr-10 ${errors.jellyfinPassword ? 'input-field-error' : ''}`}
              placeholder="Mot de passe"
            />
            <button
              type="button"
              onClick={() => setShowJellyfinPassword(!showJellyfinPassword)}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-white"
            >
              {showJellyfinPassword ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
            </button>
          </div>
        </div>
      </div>

      {/* Navigation - Always visible */}
      <div className="flex justify-between pt-4">
        <button onClick={onBack} className="btn-ghost">
          <ArrowLeft className="w-4 h-4" />
          Retour
        </button>
        <button onClick={handleNext} className="btn-primary">
          Continuer
          <ArrowRight className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}
