import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { Check, Loader2, X, Terminal } from 'lucide-react';
import { useStore, PiInfo } from '../../lib/store';

interface ConfigProgressProps {
  piInfo: PiInfo;
  onComplete: () => void;
  onError: () => void;
}

interface ConfigStep {
  id: string;
  label: string;
  status: 'pending' | 'active' | 'complete' | 'error';
}

export default function ConfigProgress({ piInfo, onComplete, onError }: ConfigProgressProps) {
  const { config, sshCredentials, addLog, setInstallationId } = useStore();
  const [steps, setSteps] = useState<ConfigStep[]>([
    { id: 'ssh', label: 'Connexion SSH', status: 'pending' },
    { id: 'update', label: 'Mise à jour système', status: 'pending' },
    { id: 'docker', label: 'Installation Docker', status: 'pending' },
    { id: 'clone', label: 'Récupération configuration', status: 'pending' },
    { id: 'compose', label: 'Démarrage des services', status: 'pending' },
    { id: 'radarr', label: 'Configuration Radarr', status: 'pending' },
    { id: 'sonarr', label: 'Configuration Sonarr', status: 'pending' },
    { id: 'prowlarr', label: 'Configuration Prowlarr', status: 'pending' },
    { id: 'jellyfin', label: 'Configuration Jellyfin', status: 'pending' },
    { id: 'jellyseerr', label: 'Configuration Jellyseerr', status: 'pending' },
    { id: 'bazarr', label: 'Configuration Bazarr', status: 'pending' },
    { id: 'final', label: 'Finalisation', status: 'pending' },
  ]);
  const [progress, setProgress] = useState(0);
  const [currentMessage, setCurrentMessage] = useState('Connexion au Pi...');
  const [error, setError] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [showLogs, setShowLogs] = useState(false);

  const appendLog = (message: string) => {
    const timestamp = new Date().toLocaleTimeString();
    setLogs((prev) => [...prev, `[${timestamp}] ${message}`]);
    addLog(message);
  };

  const updateStep = (stepId: string, status: 'active' | 'complete' | 'error') => {
    setSteps((prev) =>
      prev.map((s) => {
        if (s.id === stepId) {
          return { ...s, status };
        }
        return s;
      })
    );

    // Calculer la progression
    const completedCount = steps.filter((s) => s.status === 'complete').length;
    setProgress(Math.round((completedCount / steps.length) * 100));
  };

  useEffect(() => {
    runConfiguration();
  }, []);

  const runConfiguration = async () => {
    try {
      // 1. Test SSH
      updateStep('ssh', 'active');
      setCurrentMessage('Test de la connexion SSH...');
      appendLog(`Connexion à ${piInfo.ip}...`);

      const sshOk = await invoke<boolean>('test_ssh_connection', {
        host: piInfo.ip,
        username: 'maison',
        privateKey: sshCredentials!.privateKey,
      });

      if (!sshOk) {
        throw new Error('Impossible de se connecter en SSH');
      }

      updateStep('ssh', 'complete');
      appendLog('Connexion SSH établie');

      // 2. Installation complète
      updateStep('update', 'active');
      setCurrentMessage('Mise à jour du système...');

      await invoke('run_installation', {
        host: piInfo.ip,
        username: 'maison',
        privateKey: sshCredentials!.privateKey,
        config: {
          alldebrid_api_key: config.alldebridKey,
          jellyfin_username: config.jellyfinUsername,
          jellyfin_password: config.jellyfinPassword,
          ygg_passkey: config.yggPasskey || null,
          discord_webhook: config.discordWebhook || null,
          cloudflare_token: config.cloudflareToken || null,
        },
      });

      // Marquer les étapes comme complètes progressivement
      const stepIds = ['update', 'docker', 'clone', 'compose', 'radarr', 'sonarr', 'prowlarr', 'jellyfin', 'jellyseerr', 'bazarr', 'final'];
      for (const id of stepIds) {
        updateStep(id, 'complete');
        await new Promise((r) => setTimeout(r, 500));
      }

      // 3. Sauvegarder dans Supabase
      appendLog('Sauvegarde des informations...');

      const installId = await invoke<string>('save_to_supabase', {
        piName: piInfo.hostname,
        piIp: piInfo.ip,
        sshPublicKey: sshCredentials!.publicKey,
        sshPrivateKeyEncrypted: sshCredentials!.privateKey, // TODO: chiffrer
        installerVersion: '1.0.0',
      });

      setInstallationId(installId);
      appendLog(`Installation enregistrée: ${installId}`);

      setProgress(100);
      setCurrentMessage('Configuration terminée !');

      setTimeout(onComplete, 2000);
    } catch (err) {
      console.error('Erreur configuration:', err);
      setError(String(err));
      appendLog(`ERREUR: ${err}`);
    }
  };

  if (error) {
    return (
      <div className="space-y-6 text-center">
        <div className="w-20 h-20 mx-auto bg-red-500/20 rounded-full flex items-center justify-center">
          <X className="w-10 h-10 text-red-400" />
        </div>
        <div>
          <h2 className="text-2xl font-bold text-white mb-2">Erreur de configuration</h2>
          <p className="text-gray-400 mb-4">
            Une erreur est survenue pendant la configuration du Pi
          </p>
          <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-4 text-left max-h-48 overflow-auto">
            <p className="text-sm text-red-300 font-mono">{error}</p>
          </div>
        </div>
        <div className="flex justify-center gap-4">
          <button
            onClick={() => setShowLogs(!showLogs)}
            className="px-6 py-3 bg-gray-700 hover:bg-gray-600 text-white rounded-xl transition-colors"
          >
            Voir les logs
          </button>
          <button
            onClick={onError}
            className="px-6 py-3 bg-purple-600 hover:bg-purple-700 text-white rounded-xl transition-colors"
          >
            Réessayer
          </button>
        </div>
        {showLogs && (
          <div className="bg-gray-900 rounded-xl p-4 text-left max-h-64 overflow-auto">
            {logs.map((log, i) => (
              <p key={i} className="text-xs text-gray-400 font-mono">
                {log}
              </p>
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="text-center mb-8">
        <h2 className="text-2xl font-bold text-white mb-2">
          Configuration en cours
        </h2>
        <p className="text-gray-400">
          {piInfo.hostname} ({piInfo.ip})
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
            className="h-full bg-gradient-to-r from-green-500 to-emerald-500 transition-all duration-300"
            style={{ width: `${progress}%` }}
          />
        </div>
      </div>

      {/* Steps grid */}
      <div className="grid grid-cols-2 gap-2">
        {steps.map((step) => (
          <div
            key={step.id}
            className={`flex items-center gap-2 p-3 rounded-lg transition-colors ${
              step.status === 'active' ? 'bg-purple-500/10' : 'bg-gray-800/30'
            }`}
          >
            <div className="w-5 h-5 flex items-center justify-center">
              {step.status === 'complete' ? (
                <div className="w-4 h-4 bg-green-500 rounded-full flex items-center justify-center">
                  <Check className="w-3 h-3 text-white" />
                </div>
              ) : step.status === 'active' ? (
                <Loader2 className="w-4 h-4 text-purple-400 animate-spin" />
              ) : step.status === 'error' ? (
                <div className="w-4 h-4 bg-red-500 rounded-full flex items-center justify-center">
                  <X className="w-2 h-2 text-white" />
                </div>
              ) : (
                <div className="w-2 h-2 bg-gray-600 rounded-full" />
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

      {/* Logs toggle */}
      <button
        onClick={() => setShowLogs(!showLogs)}
        className="w-full flex items-center justify-center gap-2 py-2 text-sm text-gray-400 hover:text-white transition-colors"
      >
        <Terminal className="w-4 h-4" />
        {showLogs ? 'Masquer les logs' : 'Afficher les logs'}
      </button>

      {showLogs && (
        <div className="bg-gray-900 rounded-xl p-4 max-h-48 overflow-auto">
          {logs.map((log, i) => (
            <p key={i} className="text-xs text-gray-400 font-mono">
              {log}
            </p>
          ))}
        </div>
      )}

      {/* Time estimate */}
      <p className="text-center text-sm text-gray-500">
        Cette étape peut prendre 10-15 minutes
      </p>
    </div>
  );
}
