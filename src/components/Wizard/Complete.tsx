import { ExternalLink, Copy, Check, RotateCcw } from 'lucide-react';
import { useState } from 'react';
import { open } from '@tauri-apps/api/shell';
import { useStore, PiInfo } from '../../lib/store';

interface CompleteProps {
  piInfo: PiInfo;
  onRestart: () => void;
}

interface ServiceLink {
  name: string;
  description: string;
  port: number;
  icon: string;
}

const services: ServiceLink[] = [
  { name: 'Jellyfin', description: 'Films & Séries', port: 8096, icon: '📺' },
  { name: 'Jellyseerr', description: 'Demander du contenu', port: 5055, icon: '🎬' },
  { name: 'Radarr', description: 'Gestion films', port: 7878, icon: '🎥' },
  { name: 'Sonarr', description: 'Gestion séries', port: 8989, icon: '📺' },
  { name: 'Prowlarr', description: 'Indexeurs', port: 9696, icon: '🔍' },
  { name: 'Bazarr', description: 'Sous-titres', port: 6767, icon: '💬' },
];

export default function Complete({ piInfo, onRestart }: CompleteProps) {
  const { config } = useStore();
  const [copiedUrl, setCopiedUrl] = useState<string | null>(null);

  const copyToClipboard = async (text: string) => {
    await navigator.clipboard.writeText(text);
    setCopiedUrl(text);
    setTimeout(() => setCopiedUrl(null), 2000);
  };

  const openUrl = async (url: string) => {
    await open(url);
  };

  const getServiceUrl = (port: number) => `http://${piInfo.ip}:${port}`;

  return (
    <div className="space-y-8">
      {/* Success header */}
      <div className="text-center">
        <div className="w-24 h-24 mx-auto bg-gradient-to-br from-green-400 to-emerald-500 rounded-3xl flex items-center justify-center shadow-lg shadow-green-500/25 mb-6">
          <span className="text-5xl">🎉</span>
        </div>
        <h2 className="text-3xl font-bold text-white mb-2">
          Installation terminée !
        </h2>
        <p className="text-lg text-gray-400">
          Votre media center est prêt à l'emploi
        </p>
      </div>

      {/* Main services */}
      <div className="space-y-3">
        <h3 className="text-sm font-medium text-gray-400 px-1">
          Accédez à vos services
        </h3>

        {/* Primary: Jellyfin & Jellyseerr */}
        <div className="grid grid-cols-2 gap-3">
          {services.slice(0, 2).map((service) => (
            <div
              key={service.name}
              className="bg-gradient-to-br from-purple-500/10 to-pink-500/10 border border-purple-500/30 rounded-xl p-4"
            >
              <div className="flex items-center gap-3 mb-3">
                <div className="w-10 h-10 bg-purple-500/20 rounded-lg flex items-center justify-center">
                  <span className="text-xl">{service.icon}</span>
                </div>
                <div>
                  <p className="font-medium text-white">{service.name}</p>
                  <p className="text-xs text-gray-400">{service.description}</p>
                </div>
              </div>
              <div className="flex gap-2">
                <button
                  onClick={() => openUrl(getServiceUrl(service.port))}
                  className="flex-1 py-2 bg-purple-600 hover:bg-purple-700 text-white text-sm font-medium rounded-lg transition-colors flex items-center justify-center gap-1"
                >
                  Ouvrir
                  <ExternalLink className="w-3 h-3" />
                </button>
                <button
                  onClick={() => copyToClipboard(getServiceUrl(service.port))}
                  className="px-3 py-2 bg-gray-700 hover:bg-gray-600 text-white rounded-lg transition-colors"
                >
                  {copiedUrl === getServiceUrl(service.port) ? (
                    <Check className="w-4 h-4 text-green-400" />
                  ) : (
                    <Copy className="w-4 h-4" />
                  )}
                </button>
              </div>
            </div>
          ))}
        </div>

        {/* Secondary services */}
        <div className="bg-gray-800/50 rounded-xl divide-y divide-gray-700/50">
          {services.slice(2).map((service) => (
            <div
              key={service.name}
              className="flex items-center justify-between p-3"
            >
              <div className="flex items-center gap-3">
                <span className="text-lg">{service.icon}</span>
                <div>
                  <p className="text-sm font-medium text-white">{service.name}</p>
                  <p className="text-xs text-gray-500">{service.description}</p>
                </div>
              </div>
              <div className="flex items-center gap-2">
                <code className="text-xs text-gray-400 bg-gray-900 px-2 py-1 rounded">
                  :{service.port}
                </code>
                <button
                  onClick={() => openUrl(getServiceUrl(service.port))}
                  className="p-2 text-gray-400 hover:text-white transition-colors"
                >
                  <ExternalLink className="w-4 h-4" />
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Credentials */}
      <div className="bg-gray-800/50 rounded-xl p-6 space-y-4">
        <h3 className="font-medium text-white flex items-center gap-2">
          <span className="text-lg">👤</span>
          Vos identifiants Jellyfin
        </h3>

        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="text-xs text-gray-400 block mb-1">
              Nom d'utilisateur
            </label>
            <div className="flex items-center gap-2">
              <code className="flex-1 bg-gray-900 px-3 py-2 rounded-lg text-white">
                {config.jellyfinUsername}
              </code>
              <button
                onClick={() => copyToClipboard(config.jellyfinUsername)}
                className="p-2 text-gray-400 hover:text-white transition-colors"
              >
                {copiedUrl === config.jellyfinUsername ? (
                  <Check className="w-4 h-4 text-green-400" />
                ) : (
                  <Copy className="w-4 h-4" />
                )}
              </button>
            </div>
          </div>

          <div>
            <label className="text-xs text-gray-400 block mb-1">
              Mot de passe
            </label>
            <div className="flex items-center gap-2">
              <code className="flex-1 bg-gray-900 px-3 py-2 rounded-lg text-white">
                ••••••••
              </code>
              <button
                onClick={() => copyToClipboard(config.jellyfinPassword)}
                className="p-2 text-gray-400 hover:text-white transition-colors"
              >
                {copiedUrl === config.jellyfinPassword ? (
                  <Check className="w-4 h-4 text-green-400" />
                ) : (
                  <Copy className="w-4 h-4" />
                )}
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* Pi Info */}
      <div className="bg-green-500/10 border border-green-500/30 rounded-xl p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-3 h-3 bg-green-500 rounded-full animate-pulse" />
            <div>
              <p className="text-sm font-medium text-green-400">
                {piInfo.hostname} est en ligne
              </p>
              <p className="text-xs text-green-300/70">
                IP: {piInfo.ip}
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Actions */}
      <div className="flex justify-center gap-4">
        <button
          onClick={() => openUrl(getServiceUrl(8096))}
          className="px-8 py-3 bg-purple-600 hover:bg-purple-700 text-white font-medium rounded-xl transition-colors flex items-center gap-2"
        >
          Ouvrir Jellyfin
          <ExternalLink className="w-4 h-4" />
        </button>

        <button
          onClick={onRestart}
          className="px-6 py-3 bg-gray-700 hover:bg-gray-600 text-white rounded-xl transition-colors flex items-center gap-2"
        >
          <RotateCcw className="w-4 h-4" />
          Nouvelle installation
        </button>
      </div>

      {/* Footer note */}
      <p className="text-center text-sm text-gray-500">
        Un email récapitulatif a été envoyé à l'administrateur.
      </p>
    </div>
  );
}
