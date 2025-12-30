import { useState } from 'react';
import { ArrowLeft, ArrowRight, Eye, EyeOff, HelpCircle, ChevronDown, ChevronUp } from 'lucide-react';
import { Config } from '../../lib/store';

interface ConfigFormProps {
  config: Config;
  onConfigChange: (config: Partial<Config>) => void;
  onNext: () => void;
  onBack: () => void;
}

export default function ConfigForm({ config, onConfigChange, onNext, onBack }: ConfigFormProps) {
  const [showPassword, setShowPassword] = useState(false);
  const [showJellyfinPassword, setShowJellyfinPassword] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [errors, setErrors] = useState<Record<string, string>>({});

  const validate = (): boolean => {
    const newErrors: Record<string, string> = {};

    if (!config.wifiSSID.trim()) {
      newErrors.wifiSSID = 'Le nom du réseau WiFi est requis';
    }
    if (!config.wifiPassword.trim()) {
      newErrors.wifiPassword = 'Le mot de passe WiFi est requis';
    }
    if (!config.alldebridKey.trim()) {
      newErrors.alldebridKey = "La clé API AllDebrid est requise";
    }
    if (!config.jellyfinUsername.trim()) {
      newErrors.jellyfinUsername = "Le nom d'utilisateur Jellyfin est requis";
    }
    if (!config.jellyfinPassword.trim()) {
      newErrors.jellyfinPassword = 'Le mot de passe Jellyfin est requis';
    } else if (config.jellyfinPassword.length < 4) {
      newErrors.jellyfinPassword = 'Le mot de passe doit faire au moins 4 caractères';
    }

    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  const handleNext = () => {
    if (validate()) {
      onNext();
    }
  };

  return (
    <div className="space-y-6">
      <div className="text-center mb-8">
        <h2 className="text-2xl font-bold text-white mb-2">Configuration</h2>
        <p className="text-gray-400">
          Remplissez les informations nécessaires pour votre installation
        </p>
      </div>

      <div className="space-y-6">
        {/* WiFi Section */}
        <div className="bg-gray-800/50 rounded-xl p-6 space-y-4">
          <h3 className="text-lg font-medium text-white flex items-center gap-2">
            <span className="text-xl">📡</span>
            Réseau WiFi
          </h3>

          <div>
            <label className="block text-sm text-gray-400 mb-2">
              Nom du réseau (SSID)
            </label>
            <input
              type="text"
              value={config.wifiSSID}
              onChange={(e) => onConfigChange({ wifiSSID: e.target.value })}
              className={`w-full px-4 py-3 bg-gray-900 border ${
                errors.wifiSSID ? 'border-red-500' : 'border-gray-700'
              } rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-purple-500 transition-colors`}
              placeholder="MonWiFi"
            />
            {errors.wifiSSID && (
              <p className="mt-1 text-sm text-red-400">{errors.wifiSSID}</p>
            )}
          </div>

          <div>
            <label className="block text-sm text-gray-400 mb-2">
              Mot de passe WiFi
            </label>
            <div className="relative">
              <input
                type={showPassword ? 'text' : 'password'}
                value={config.wifiPassword}
                onChange={(e) => onConfigChange({ wifiPassword: e.target.value })}
                className={`w-full px-4 py-3 pr-12 bg-gray-900 border ${
                  errors.wifiPassword ? 'border-red-500' : 'border-gray-700'
                } rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-purple-500 transition-colors`}
                placeholder="••••••••"
              />
              <button
                type="button"
                onClick={() => setShowPassword(!showPassword)}
                className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white transition-colors"
              >
                {showPassword ? <EyeOff className="w-5 h-5" /> : <Eye className="w-5 h-5" />}
              </button>
            </div>
            {errors.wifiPassword && (
              <p className="mt-1 text-sm text-red-400">{errors.wifiPassword}</p>
            )}
          </div>
        </div>

        {/* AllDebrid Section */}
        <div className="bg-gray-800/50 rounded-xl p-6 space-y-4">
          <h3 className="text-lg font-medium text-white flex items-center gap-2">
            <span className="text-xl">🎬</span>
            AllDebrid
            <span className="text-xs bg-red-500/20 text-red-400 px-2 py-0.5 rounded-full">
              Obligatoire
            </span>
          </h3>

          <div>
            <label className="block text-sm text-gray-400 mb-2">
              Clé API AllDebrid
            </label>
            <input
              type="text"
              value={config.alldebridKey}
              onChange={(e) => onConfigChange({ alldebridKey: e.target.value })}
              className={`w-full px-4 py-3 bg-gray-900 border ${
                errors.alldebridKey ? 'border-red-500' : 'border-gray-700'
              } rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-purple-500 transition-colors font-mono`}
              placeholder="xxxxxxxxxxxxxxxxxxxxxxxx"
            />
            {errors.alldebridKey && (
              <p className="mt-1 text-sm text-red-400">{errors.alldebridKey}</p>
            )}
          </div>

          <a
            href="https://alldebrid.com/apikeys/"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1 text-sm text-purple-400 hover:text-purple-300 transition-colors"
          >
            <HelpCircle className="w-4 h-4" />
            Comment obtenir ma clé API ?
          </a>
        </div>

        {/* Jellyfin Account Section */}
        <div className="bg-gray-800/50 rounded-xl p-6 space-y-4">
          <h3 className="text-lg font-medium text-white flex items-center gap-2">
            <span className="text-xl">👤</span>
            Compte Jellyfin
          </h3>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-sm text-gray-400 mb-2">
                Nom d'utilisateur
              </label>
              <input
                type="text"
                value={config.jellyfinUsername}
                onChange={(e) => onConfigChange({ jellyfinUsername: e.target.value })}
                className={`w-full px-4 py-3 bg-gray-900 border ${
                  errors.jellyfinUsername ? 'border-red-500' : 'border-gray-700'
                } rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-purple-500 transition-colors`}
                placeholder="papa"
              />
              {errors.jellyfinUsername && (
                <p className="mt-1 text-sm text-red-400">{errors.jellyfinUsername}</p>
              )}
            </div>

            <div>
              <label className="block text-sm text-gray-400 mb-2">
                Mot de passe
              </label>
              <div className="relative">
                <input
                  type={showJellyfinPassword ? 'text' : 'password'}
                  value={config.jellyfinPassword}
                  onChange={(e) => onConfigChange({ jellyfinPassword: e.target.value })}
                  className={`w-full px-4 py-3 pr-12 bg-gray-900 border ${
                    errors.jellyfinPassword ? 'border-red-500' : 'border-gray-700'
                  } rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-purple-500 transition-colors`}
                  placeholder="••••••••"
                />
                <button
                  type="button"
                  onClick={() => setShowJellyfinPassword(!showJellyfinPassword)}
                  className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white transition-colors"
                >
                  {showJellyfinPassword ? <EyeOff className="w-5 h-5" /> : <Eye className="w-5 h-5" />}
                </button>
              </div>
              {errors.jellyfinPassword && (
                <p className="mt-1 text-sm text-red-400">{errors.jellyfinPassword}</p>
              )}
            </div>
          </div>
        </div>

        {/* Advanced Options */}
        <div className="bg-gray-800/30 rounded-xl overflow-hidden">
          <button
            onClick={() => setShowAdvanced(!showAdvanced)}
            className="w-full px-6 py-4 flex items-center justify-between text-gray-400 hover:text-white transition-colors"
          >
            <span className="font-medium">Options avancées (facultatif)</span>
            {showAdvanced ? (
              <ChevronUp className="w-5 h-5" />
            ) : (
              <ChevronDown className="w-5 h-5" />
            )}
          </button>

          {showAdvanced && (
            <div className="px-6 pb-6 space-y-4">
              <div>
                <label className="block text-sm text-gray-400 mb-2">
                  YGG Passkey (indexeur torrent)
                </label>
                <input
                  type="text"
                  value={config.yggPasskey || ''}
                  onChange={(e) => onConfigChange({ yggPasskey: e.target.value })}
                  className="w-full px-4 py-3 bg-gray-900 border border-gray-700 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-purple-500 transition-colors font-mono"
                  placeholder="Optionnel"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-2">
                  Discord Webhook (notifications)
                </label>
                <input
                  type="text"
                  value={config.discordWebhook || ''}
                  onChange={(e) => onConfigChange({ discordWebhook: e.target.value })}
                  className="w-full px-4 py-3 bg-gray-900 border border-gray-700 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-purple-500 transition-colors font-mono"
                  placeholder="https://discord.com/api/webhooks/..."
                />
              </div>

              <div>
                <label className="block text-sm text-gray-400 mb-2">
                  Cloudflare Tunnel Token (accès distant)
                </label>
                <input
                  type="text"
                  value={config.cloudflareToken || ''}
                  onChange={(e) => onConfigChange({ cloudflareToken: e.target.value })}
                  className="w-full px-4 py-3 bg-gray-900 border border-gray-700 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-purple-500 transition-colors font-mono"
                  placeholder="Optionnel"
                />
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Navigation */}
      <div className="flex justify-between pt-4">
        <button
          onClick={onBack}
          className="inline-flex items-center gap-2 px-6 py-3 text-gray-400 hover:text-white transition-colors"
        >
          <ArrowLeft className="w-5 h-5" />
          Retour
        </button>

        <button
          onClick={handleNext}
          className="inline-flex items-center gap-2 px-8 py-3 bg-purple-600 hover:bg-purple-700 text-white font-medium rounded-xl transition-colors"
        >
          Suivant
          <ArrowRight className="w-5 h-5" />
        </button>
      </div>
    </div>
  );
}
