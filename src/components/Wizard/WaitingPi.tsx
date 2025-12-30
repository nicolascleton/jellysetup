import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { ArrowLeft, Loader2, Wifi, RefreshCw, CheckCircle2 } from 'lucide-react';
import { useStore, PiInfo } from '../../lib/store';

interface WaitingPiProps {
  onPiFound: (info: PiInfo) => void;
  onBack: () => void;
}

export default function WaitingPi({ onPiFound, onBack }: WaitingPiProps) {
  const { config, addLog } = useStore();
  const [attempts, setAttempts] = useState(0);
  const maxAttempts = 60;

  useEffect(() => {
    const searchInterval = setInterval(async () => {
      try {
        addLog(`Recherche du Pi (tentative ${attempts + 1})...`);
        const piInfo = await invoke<PiInfo | null>('discover_pi', {
          hostname: config.hostname || 'jellypi',
          timeout_secs: 5,
        });
        if (piInfo) {
          addLog(`Pi trouvé: ${piInfo.ip}`);
          clearInterval(searchInterval);
          onPiFound(piInfo);
        } else {
          setAttempts((prev) => prev + 1);
        }
      } catch {
        setAttempts((prev) => prev + 1);
      }
    }, 5000);
    return () => clearInterval(searchInterval);
  }, [config.hostname, attempts]);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="text-center">
        <div className="w-16 h-16 mx-auto bg-gradient-to-br from-green-400 to-emerald-500 rounded-2xl flex items-center justify-center mb-4">
          <CheckCircle2 className="w-8 h-8 text-white" />
        </div>
        <h3 className="text-lg font-semibold text-white mb-1">Carte SD prête !</h3>
      </div>

      {/* Instructions compactes */}
      <div className="grid grid-cols-2 gap-2 text-sm">
        {['1. Retirez la carte', '2. Insérez dans le Pi', '3. Branchez le Pi', '4. Patientez 2min'].map((text, i) => (
          <div key={i} className="card !p-3 text-center text-zinc-300">{text}</div>
        ))}
      </div>

      {/* Status */}
      <div className="card !p-4 flex items-center gap-4">
        <Loader2 className="w-6 h-6 text-blue-400 animate-spin flex-shrink-0" />
        <div className="flex-1">
          <p className="text-sm text-white">Recherche du Pi...</p>
          <div className="flex items-center gap-2 mt-1">
            <div className="flex-1 h-1 bg-zinc-800 rounded-full">
              <div className="h-full bg-blue-500 rounded-full" style={{ width: `${(attempts / maxAttempts) * 100}%` }} />
            </div>
            <span className="text-xs text-zinc-500">{attempts}/{maxAttempts}</span>
          </div>
        </div>
      </div>

      {/* Tip */}
      <div className="flex items-center gap-2 p-3 bg-blue-500/10 rounded-xl text-sm">
        <Wifi className="w-4 h-4 text-blue-400" />
        <span className="text-blue-300/70">Même réseau WiFi requis</span>
      </div>

      {/* Navigation */}
      <div className="flex justify-between pt-2">
        <button onClick={onBack} className="btn-ghost">
          <ArrowLeft className="w-4 h-4" />
          Retour
        </button>
        {attempts >= maxAttempts && (
          <button onClick={() => setAttempts(0)} className="btn-primary">
            <RefreshCw className="w-4 h-4" />
            Réessayer
          </button>
        )}
      </div>
    </div>
  );
}
