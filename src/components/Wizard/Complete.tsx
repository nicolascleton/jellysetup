import { ExternalLink, Copy, Check, RotateCcw, PartyPopper } from 'lucide-react';
import { useState } from 'react';
import { open } from '@tauri-apps/api/shell';
import { useStore, PiInfo } from '../../lib/store';

interface CompleteProps {
  piInfo: PiInfo;
  onRestart: () => void;
}

const services = [
  { name: 'Jellyfin', port: 8096, icon: '📺' },
  { name: 'Jellyseerr', port: 5055, icon: '🎬' },
];

export default function Complete({ piInfo, onRestart }: CompleteProps) {
  const { config } = useStore();
  const [copied, setCopied] = useState<string | null>(null);

  const copy = async (text: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(text);
    setTimeout(() => setCopied(null), 2000);
  };

  const getUrl = (port: number) => `http://${piInfo.ip}:${port}`;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="text-center">
        <div className="w-20 h-20 mx-auto bg-gradient-to-br from-green-400 to-emerald-500 rounded-3xl flex items-center justify-center mb-4 shadow-lg shadow-green-500/30">
          <PartyPopper className="w-10 h-10 text-white" />
        </div>
        <h2 className="text-2xl font-bold text-white mb-1">Terminé !</h2>
        <p className="text-sm text-zinc-400">Votre media center est prêt</p>
      </div>

      {/* Services */}
      <div className="grid grid-cols-2 gap-3">
        {services.map((s) => (
          <div key={s.name} className="card !p-4">
            <div className="flex items-center gap-2 mb-3">
              <span className="text-xl">{s.icon}</span>
              <span className="font-medium text-white text-sm">{s.name}</span>
            </div>
            <button
              onClick={() => open(getUrl(s.port))}
              className="w-full py-2 bg-purple-600 hover:bg-purple-500 text-white text-sm font-medium rounded-lg flex items-center justify-center gap-2"
            >
              Ouvrir <ExternalLink className="w-3 h-3" />
            </button>
          </div>
        ))}
      </div>

      {/* Credentials */}
      <div className="card !p-4 space-y-3">
        <span className="text-sm text-zinc-400">Identifiants Jellyfin</span>
        <div className="grid grid-cols-2 gap-3">
          <div className="bg-zinc-800/50 rounded-lg p-2 flex items-center justify-between">
            <code className="text-sm text-white truncate">{config.jellyfinUsername}</code>
            <button onClick={() => copy(config.jellyfinUsername)} className="p-1 text-zinc-400 hover:text-white">
              {copied === config.jellyfinUsername ? <Check className="w-3 h-3 text-green-400" /> : <Copy className="w-3 h-3" />}
            </button>
          </div>
          <div className="bg-zinc-800/50 rounded-lg p-2 flex items-center justify-between">
            <code className="text-sm text-white">••••••••</code>
            <button onClick={() => copy(config.jellyfinPassword)} className="p-1 text-zinc-400 hover:text-white">
              {copied === config.jellyfinPassword ? <Check className="w-3 h-3 text-green-400" /> : <Copy className="w-3 h-3" />}
            </button>
          </div>
        </div>
      </div>

      {/* Status */}
      <div className="flex items-center gap-2 p-3 bg-green-500/10 rounded-xl">
        <div className="w-2 h-2 bg-green-500 rounded-full animate-pulse" />
        <span className="text-sm text-green-400">{piInfo.hostname}</span>
        <span className="text-xs text-zinc-500 font-mono">{piInfo.ip}</span>
      </div>

      {/* Actions */}
      <div className="flex justify-center gap-3">
        <button onClick={() => open(getUrl(8096))} className="btn-primary">
          Ouvrir Jellyfin <ExternalLink className="w-4 h-4" />
        </button>
        <button onClick={onRestart} className="btn-secondary">
          <RotateCcw className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}
