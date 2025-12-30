import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';
import { Check, Loader2, HardDrive, AlertTriangle } from 'lucide-react';
import { useStore } from '../../lib/store';

interface FlashProgressProps {
  onComplete: () => void;
  onError: () => void;
}

interface FlashStep {
  id: string;
  label: string;
  status: 'pending' | 'active' | 'complete' | 'error';
}

interface ProgressEvent {
  step: string;
  percent: number;
  message: string;
  speed?: string;
}

export default function FlashProgress({ onComplete, onError }: FlashProgressProps) {
  const { config, selectedSD, setSSHCredentials, addLog } = useStore();
  const [steps, setSteps] = useState<FlashStep[]>([
    { id: 'download', label: 'Téléchargement', status: 'pending' },
    { id: 'write', label: 'Écriture', status: 'pending' },
    { id: 'configure', label: 'Configuration', status: 'pending' },
    { id: 'eject', label: 'Éjection', status: 'pending' },
  ]);
  const [progress, setProgress] = useState(0);
  const [currentMessage, setCurrentMessage] = useState('Préparation...');
  const [error, setError] = useState<string | null>(null);

  // Protection contre les lancements multiples (React StrictMode, remount, etc.)
  const hasStarted = useRef(false);

  useEffect(() => {
    // Ne lancer qu'une seule fois, même si le composant est remonté
    if (hasStarted.current) {
      console.log('[FlashProgress] Already started, skipping...');
      return;
    }
    hasStarted.current = true;
    console.log('[FlashProgress] Starting flash (first time)');

    startFlashing();
    const unlisten = listen<ProgressEvent>('flash-progress', (event) => {
      const { step, percent, message } = event.payload;
      setProgress(percent);
      setCurrentMessage(message);
      setSteps((prev) =>
        prev.map((s) => {
          if (s.id === step) return { ...s, status: 'active' };
          else if (prev.findIndex((x) => x.id === step) > prev.findIndex((x) => x.id === s.id))
            return { ...s, status: 'complete' };
          return s;
        })
      );
      addLog(message);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const startFlashing = async () => {
    try {
      addLog('Génération des clés SSH...');
      const sshKeys = await invoke<{ public_key: string; private_key: string }>('generate_ssh_keys');
      setSSHCredentials({ publicKey: sshKeys.public_key, privateKey: sshKeys.private_key });

      await invoke('flash_sd_card', {
        config: {
          sdPath: selectedSD!.path,
          // Système
          hostname: config.hostname || 'jellypi',
          systemUsername: config.systemUsername || 'maison',
          systemPassword: config.systemPassword,
          // WiFi
          wifiSsid: config.wifiSSID,
          wifiPassword: config.wifiPassword,
          wifiCountry: config.wifiCountry || 'FR',
          // Locale
          timezone: config.timezone || 'Europe/Paris',
          keymap: config.keymap || 'fr',
        },
        sshPublicKey: sshKeys.public_key,
      });

      setSteps((prev) => prev.map((s) => ({ ...s, status: 'complete' })));
      setProgress(100);
      setCurrentMessage('Carte SD prête !');
      setTimeout(onComplete, 1500);
    } catch (err) {
      setError(String(err));
      addLog(`ERREUR: ${err}`);
    }
  };

  if (error) {
    return (
      <div className="text-center space-y-4">
        <div className="w-16 h-16 mx-auto bg-red-500/20 rounded-2xl flex items-center justify-center">
          <AlertTriangle className="w-8 h-8 text-red-400" />
        </div>
        <div>
          <h3 className="text-lg font-semibold text-white mb-1">Erreur</h3>
          <p className="text-sm text-red-300/80 font-mono">{error}</p>
        </div>
        <button onClick={onError} className="btn-primary">Réessayer</button>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="text-center">
        <div className="w-16 h-16 mx-auto bg-gradient-to-br from-purple-500 to-pink-500 rounded-2xl flex items-center justify-center mb-4 animate-pulse-glow">
          <HardDrive className="w-8 h-8 text-white" />
        </div>
        <p className="text-sm text-zinc-400">{currentMessage}</p>
      </div>

      {/* Progress */}
      <div className="space-y-2">
        <div className="flex justify-between text-sm">
          <span className="text-zinc-500">Progression</span>
          <span className="text-white font-medium">{progress}%</span>
        </div>
        <div className="progress-bar">
          <div className="progress-fill" style={{ width: `${progress}%` }} />
        </div>
      </div>

      {/* Steps */}
      <div className="flex justify-between">
        {steps.map((step) => (
          <div key={step.id} className="flex flex-col items-center gap-1">
            <div className={`w-8 h-8 rounded-lg flex items-center justify-center ${
              step.status === 'complete' ? 'bg-green-500/20' :
              step.status === 'active' ? 'bg-purple-500/20' : 'bg-zinc-800'
            }`}>
              {step.status === 'complete' ? <Check className="w-4 h-4 text-green-400" /> :
               step.status === 'active' ? <Loader2 className="w-4 h-4 text-purple-400 animate-spin" /> :
               <div className="w-2 h-2 bg-zinc-600 rounded-full" />}
            </div>
            <span className={`text-xs ${
              step.status === 'complete' ? 'text-green-400' :
              step.status === 'active' ? 'text-white' : 'text-zinc-500'
            }`}>{step.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
