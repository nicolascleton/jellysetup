import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';
import { Check, Loader2, X } from 'lucide-react';
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
    { id: 'download', label: 'Téléchargement Raspberry Pi OS', status: 'pending' },
    { id: 'verify', label: 'Vérification intégrité', status: 'pending' },
    { id: 'extract', label: 'Extraction de l\'image', status: 'pending' },
    { id: 'unmount', label: 'Démontage de la carte', status: 'pending' },
    { id: 'write', label: 'Écriture sur la carte SD', status: 'pending' },
    { id: 'configure', label: 'Configuration SSH et WiFi', status: 'pending' },
    { id: 'eject', label: 'Éjection de la carte', status: 'pending' },
  ]);
  const [progress, setProgress] = useState(0);
  const [currentMessage, setCurrentMessage] = useState('Préparation...');
  const [speed, setSpeed] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    startFlashing();

    // Écouter les événements de progression
    const unlisten = listen<ProgressEvent>('flash-progress', (event) => {
      const { step, percent, message, speed } = event.payload;

      setProgress(percent);
      setCurrentMessage(message);
      if (speed) setSpeed(speed);

      // Mettre à jour le statut des étapes
      setSteps((prev) =>
        prev.map((s) => {
          if (s.id === step) {
            return { ...s, status: 'active' };
          } else if (
            prev.findIndex((x) => x.id === step) > prev.findIndex((x) => x.id === s.id)
          ) {
            return { ...s, status: 'complete' };
          }
          return s;
        })
      );

      addLog(message);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const startFlashing = async () => {
    try {
      // 1. Générer les clés SSH
      addLog('Génération des clés SSH...');
      const sshKeys = await invoke<{ public_key: string; private_key: string }>(
        'generate_ssh_keys'
      );
      setSSHCredentials({
        publicKey: sshKeys.public_key,
        privateKey: sshKeys.private_key,
      });
      addLog('Clés SSH générées');

      // 2. Flasher la carte SD
      await invoke('flash_sd_card', {
        config: {
          sd_path: selectedSD!.path,
          wifi_ssid: config.wifiSSID,
          wifi_password: config.wifiPassword,
          hostname: config.hostname || 'jellypi',
          timezone: 'Europe/Paris',
        },
        ssh_public_key: sshKeys.public_key,
      });

      // Marquer toutes les étapes comme complètes
      setSteps((prev) => prev.map((s) => ({ ...s, status: 'complete' })));
      setProgress(100);
      setCurrentMessage('Carte SD prête !');

      // Attendre un peu avant de passer à l'étape suivante
      setTimeout(onComplete, 2000);
    } catch (err) {
      console.error('Erreur flash:', err);
      setError(String(err));
      addLog(`ERREUR: ${err}`);
    }
  };

  if (error) {
    return (
      <div className="space-y-6 text-center">
        <div className="w-20 h-20 mx-auto bg-red-500/20 rounded-full flex items-center justify-center">
          <X className="w-10 h-10 text-red-400" />
        </div>
        <div>
          <h2 className="text-2xl font-bold text-white mb-2">Erreur</h2>
          <p className="text-gray-400 mb-4">
            Une erreur est survenue pendant l'écriture de la carte SD
          </p>
          <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-4 text-left">
            <p className="text-sm text-red-300 font-mono">{error}</p>
          </div>
        </div>
        <button
          onClick={onError}
          className="px-8 py-3 bg-gray-700 hover:bg-gray-600 text-white font-medium rounded-xl transition-colors"
        >
          Réessayer
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="text-center mb-8">
        <h2 className="text-2xl font-bold text-white mb-2">
          Installation en cours
        </h2>
        <p className="text-gray-400">
          Veuillez ne pas retirer la carte SD pendant l'opération
        </p>
      </div>

      {/* Progress bar */}
      <div className="space-y-2">
        <div className="flex justify-between text-sm">
          <span className="text-gray-400">{currentMessage}</span>
          <span className="text-white font-medium">{progress}%</span>
        </div>
        <div className="h-3 bg-gray-800 rounded-full overflow-hidden">
          <div
            className="h-full bg-gradient-to-r from-purple-500 to-pink-500 transition-all duration-300"
            style={{ width: `${progress}%` }}
          />
        </div>
        {speed && (
          <p className="text-xs text-gray-500 text-right">
            Vitesse: {speed}
          </p>
        )}
      </div>

      {/* Steps list */}
      <div className="bg-gray-800/50 rounded-xl p-4 space-y-2">
        {steps.map((step) => (
          <div
            key={step.id}
            className={`flex items-center gap-3 p-2 rounded-lg transition-colors ${
              step.status === 'active' ? 'bg-purple-500/10' : ''
            }`}
          >
            <div className="w-6 h-6 flex items-center justify-center">
              {step.status === 'complete' ? (
                <div className="w-5 h-5 bg-green-500 rounded-full flex items-center justify-center">
                  <Check className="w-3 h-3 text-white" />
                </div>
              ) : step.status === 'active' ? (
                <Loader2 className="w-5 h-5 text-purple-400 animate-spin" />
              ) : step.status === 'error' ? (
                <div className="w-5 h-5 bg-red-500 rounded-full flex items-center justify-center">
                  <X className="w-3 h-3 text-white" />
                </div>
              ) : (
                <div className="w-3 h-3 bg-gray-600 rounded-full" />
              )}
            </div>
            <span
              className={`text-sm ${
                step.status === 'complete'
                  ? 'text-green-400'
                  : step.status === 'active'
                  ? 'text-white'
                  : step.status === 'error'
                  ? 'text-red-400'
                  : 'text-gray-500'
              }`}
            >
              {step.label}
            </span>
          </div>
        ))}
      </div>

      {/* Estimated time */}
      <p className="text-center text-sm text-gray-500">
        Temps estimé: 5-10 minutes selon la vitesse de votre carte SD
      </p>
    </div>
  );
}
