import { useState } from 'react';
import { ArrowLeft, ArrowRight, Eye, EyeOff, Key, User, Monitor, Info, ExternalLink } from 'lucide-react';
import { open } from '@tauri-apps/api/shell';
import { Config } from '../../lib/store';

interface QuickConnectProps {
  config: Config;
  onConfigChange: (config: Partial<Config>) => void;
  onNext: () => void;
  onBack: () => void;
}

export default function QuickConnect({ config, onConfigChange, onNext, onBack }: QuickConnectProps) {
  const [showSystemPassword, setShowSystemPassword] = useState(false);
  const [showJellyfinPassword, setShowJellyfinPassword] = useState(false);

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="text-center mb-4">
        <h3 className="text-lg font-semibold text-white">Connexion à un Pi existant</h3>
        <p className="text-sm text-zinc-400">Entrez les informations de votre Raspberry Pi</p>
      </div>

      {/* Pi System */}
      <div className="card !p-4">
        <div className="flex items-center gap-2 mb-3">
          <Monitor className="w-4 h-4 text-green-400" />
          <span className="font-medium text-white text-sm">Système Raspberry Pi</span>
        </div>
        <p className="text-xs text-zinc-500 mb-3 flex items-center gap-1">
          <Info className="w-3 h-3" />
          Les identifiants que vous avez configurés sur le Pi
        </p>
        <div className="grid grid-cols-3 gap-3">
          <div>
            <input
              type="text"
              value={config.hostname}
              onChange={(e) => onConfigChange({ hostname: e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, '') })}
              className="input-field text-sm py-2.5"
              placeholder="Hostname (ex: maison)"
            />
          </div>
          <div>
            <input
              type="text"
              value={config.systemUsername}
              onChange={(e) => onConfigChange({ systemUsername: e.target.value.toLowerCase().replace(/[^a-z0-9_]/g, '') })}
              className="input-field text-sm py-2.5"
              placeholder="Utilisateur"
            />
          </div>
          <div className="relative">
            <input
              type={showSystemPassword ? 'text' : 'password'}
              value={config.systemPassword}
              onChange={(e) => onConfigChange({ systemPassword: e.target.value })}
              className="input-field text-sm py-2.5 pr-10"
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
      </div>

      {/* AllDebrid */}
      <div className="card !p-4">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <Key className="w-4 h-4 text-red-400" />
            <span className="font-medium text-white text-sm">AllDebrid API</span>
          </div>
          <button
            onClick={() => open('https://alldebrid.com/apikeys/')}
            className="text-xs text-purple-400 hover:text-purple-300 flex items-center gap-1"
          >
            Obtenir la clé <ExternalLink className="w-3 h-3" />
          </button>
        </div>
        <input
          type="text"
          value={config.alldebridKey}
          onChange={(e) => onConfigChange({ alldebridKey: e.target.value })}
          className="input-field text-sm py-2.5 font-mono"
          placeholder="Votre clé API AllDebrid"
        />
      </div>

      {/* YGG Passkey (optional) */}
      <div className="card !p-4">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <Key className="w-4 h-4 text-yellow-400" />
            <span className="font-medium text-white text-sm">YGG Passkey</span>
            <span className="text-xs text-zinc-500">(optionnel)</span>
          </div>
          <button
            onClick={() => open('https://yggapi.eu/')}
            className="text-xs text-yellow-400 hover:text-yellow-300 flex items-center gap-1"
          >
            Obtenir la passkey <ExternalLink className="w-3 h-3" />
          </button>
        </div>
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
        <div className="flex items-center gap-2 mb-2">
          <User className="w-4 h-4 text-purple-400" />
          <span className="font-medium text-white text-sm">Compte Jellyfin</span>
        </div>
        <p className="text-xs text-zinc-500 mb-3 flex items-center gap-1">
          <Info className="w-3 h-3" />
          Le compte sera créé automatiquement sur Jellyfin
        </p>
        <div className="grid grid-cols-2 gap-3">
          <input
            type="text"
            value={config.jellyfinUsername}
            onChange={(e) => onConfigChange({ jellyfinUsername: e.target.value })}
            className="input-field text-sm py-2.5"
            placeholder="Nom d'utilisateur"
          />
          <div className="relative">
            <input
              type={showJellyfinPassword ? 'text' : 'password'}
              value={config.jellyfinPassword}
              onChange={(e) => onConfigChange({ jellyfinPassword: e.target.value })}
              className="input-field text-sm py-2.5 pr-10"
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

      {/* Navigation */}
      <div className="flex justify-between pt-4">
        <button onClick={onBack} className="btn-ghost">
          <ArrowLeft className="w-4 h-4" />
          Retour
        </button>
        <button
          type="button"
          onClick={() => {
            try {
              console.log('[QuickConnect] Button clicked, calling onNext');
              onNext();
              console.log('[QuickConnect] onNext completed');
            } catch (error) {
              console.error('[QuickConnect] Error in onNext:', error);
              alert('Erreur: ' + String(error));
            }
          }}
          className="btn-primary"
        >
          Rechercher le Pi
          <ArrowRight className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}
