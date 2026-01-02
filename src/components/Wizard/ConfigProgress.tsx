import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';
import { Check, Loader2, Cpu, RefreshCw, AlertTriangle } from 'lucide-react';
import { useStore, PiInfo, JellyfinAuth } from '../../lib/store';

interface ConfigProgressProps {
  piInfo: PiInfo;
  onComplete: () => void;
  onError: () => void;
}

export default function ConfigProgress({ piInfo, onComplete, onError }: ConfigProgressProps) {
  const { config, sshCredentials, addLog, setInstallationId, setJellyfinAuth } = useStore();
  const [steps] = useState([
    'Connexion SSH', 'Mise à jour', 'Docker', 'Reboot', 'Structure', 'Docker Compose', 'Services', 'Configuration', 'Finalisation'
  ]);
  const [currentStep, setCurrentStep] = useState(0);
  const [progress, setProgress] = useState(0);
  const [statusMessage, setStatusMessage] = useState('Démarrage...');
  const [error, setError] = useState<string | null>(null);

  // Protection contre les lancements multiples (React StrictMode)
  const hasStarted = useRef(false);

  // Map des étapes backend vers frontend
  const stepMap: Record<string, number> = {
    'update': 1,
    'docker': 2,
    'reboot': 3,
    'structure': 4,
    'compose_write': 5,
    'compose_up': 6,
    'wait_services': 7,
    'config': 8,
    'complete': 8,
  };

  useEffect(() => {
    // Écouter les événements de progression du backend
    const unlisten = listen<{ step: string; percent: number; message: string; jellyfin_auth?: JellyfinAuth }>('flash-progress', (event) => {
      console.log('[ConfigProgress] Progress event:', event.payload);
      const { step, percent, message, jellyfin_auth } = event.payload;

      setProgress(percent);
      setStatusMessage(message);

      if (stepMap[step] !== undefined) {
        setCurrentStep(stepMap[step]);
      }

      // Capturer les données d'auth Jellyfin pour auto-login
      if (jellyfin_auth) {
        console.log('[ConfigProgress] Jellyfin auth received:', jellyfin_auth);
        setJellyfinAuth(jellyfin_auth);
      }

      addLog(message);
    });

    return () => {
      unlisten.then(fn => fn());
    };
  }, []);

  useEffect(() => {
    if (hasStarted.current) {
      console.log('[ConfigProgress] Already started, skipping duplicate...');
      return;
    }
    hasStarted.current = true;
    console.log('[ConfigProgress] Starting installation (first time only)');
    runConfiguration();
  }, []);

  const runConfiguration = async () => {
    try {
      setCurrentStep(0);
      addLog(`Connexion à ${piInfo.ip}...`);

      // Utiliser mot de passe si pas de clés SSH (flow QuickConnect)
      const usePassword = !sshCredentials;
      const username = config.systemUsername || 'maison';

      if (usePassword) {
        addLog(`Authentification par mot de passe pour ${username}@${piInfo.ip}`);
        const sshOk = await invoke<boolean>('test_ssh_connection_password', {
          host: piInfo.ip,
          username: username,
          password: config.systemPassword,
        });
        if (!sshOk) throw new Error('Connexion SSH impossible - vérifiez le mot de passe');
      } else {
        const sshOk = await invoke<boolean>('test_ssh_connection', {
          host: piInfo.ip,
          username: username,
          privateKey: sshCredentials.privateKey,
        });
        if (!sshOk) throw new Error('Connexion SSH impossible');
      }

      setCurrentStep(1);
      setProgress(12);

      // Installation avec mot de passe ou clé
      if (usePassword) {
        await invoke('run_installation_password', {
          host: piInfo.ip,
          username: username,
          password: config.systemPassword,
          config: {
            alldebrid_api_key: config.alldebridKey,
            jellyfin_username: config.jellyfinUsername,
            jellyfin_password: config.jellyfinPassword,
            jellyfin_server_name: config.jellyfinServerName || config.hostname,
            admin_email: config.adminEmail || null,
            ygg_passkey: config.yggPasskey || null,
            discord_webhook: config.discordWebhook || null,
            cloudflare_token: config.cloudflareToken || null,
          },
        });
      } else {
        await invoke('run_installation', {
          host: piInfo.ip,
          username: username,
          privateKey: sshCredentials.privateKey,
          config: {
            alldebrid_api_key: config.alldebridKey,
            jellyfin_username: config.jellyfinUsername,
            jellyfin_password: config.jellyfinPassword,
            jellyfin_server_name: config.jellyfinServerName || config.hostname,
            admin_email: config.adminEmail || null,
            ygg_passkey: config.yggPasskey || null,
            discord_webhook: config.discordWebhook || null,
            cloudflare_token: config.cloudflareToken || null,
          },
        });
      }

      // Installation terminée - sauvegarder dans Supabase
      setCurrentStep(steps.length - 1);
      setProgress(100);
      setStatusMessage('Installation terminée !');

      const installId = await invoke<string>('save_to_supabase', {
        piName: piInfo.hostname,
        piIp: piInfo.ip,
        sshPublicKey: sshCredentials?.publicKey || '',
        sshPrivateKeyEncrypted: sshCredentials?.privateKey || '',
        installerVersion: '1.0.0',
      });
      setInstallationId(installId);
      setProgress(100);
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
          <p className="text-sm text-red-300/80">{error}</p>
        </div>
        <button onClick={onError} className="btn-primary">
          <RefreshCw className="w-4 h-4" />
          Réessayer
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="text-center">
        <div className="w-16 h-16 mx-auto bg-gradient-to-br from-green-400 to-emerald-500 rounded-2xl flex items-center justify-center mb-4 animate-pulse-glow" style={{ boxShadow: '0 0 30px rgba(34, 197, 94, 0.4)' }}>
          <Cpu className="w-8 h-8 text-white" />
        </div>
        <p className="text-sm text-zinc-400">{piInfo.hostname} • {piInfo.ip}</p>
      </div>

      {/* Progress */}
      <div className="space-y-2">
        <div className="flex justify-between text-sm">
          <span className="text-zinc-400">{statusMessage}</span>
          <span className="text-white font-medium">{progress}%</span>
        </div>
        <div className="h-2 bg-zinc-800 rounded-full overflow-hidden">
          <div className="h-full bg-gradient-to-r from-green-500 to-emerald-400 rounded-full transition-all duration-500" style={{ width: `${progress}%` }} />
        </div>
        <p className="text-xs text-zinc-500 text-center">{steps[currentStep]}</p>
      </div>

      {/* Steps grid */}
      <div className="grid grid-cols-4 sm:grid-cols-6 gap-1.5">
        {steps.map((step, i) => (
          <div key={step} className={`p-2 rounded-lg text-center ${
            i < currentStep ? 'bg-green-500/10' :
            i === currentStep ? 'bg-purple-500/10 border border-purple-500/30' : 'bg-zinc-800/30'
          }`}>
            <div className={`w-6 h-6 mx-auto mb-1 rounded-lg flex items-center justify-center ${
              i < currentStep ? 'bg-green-500/20' :
              i === currentStep ? 'bg-purple-500/20' : 'bg-zinc-800'
            }`}>
              {i < currentStep ? <Check className="w-3 h-3 text-green-400" /> :
               i === currentStep ? <Loader2 className="w-3 h-3 text-purple-400 animate-spin" /> :
               <div className="w-1.5 h-1.5 bg-zinc-600 rounded-full" />}
            </div>
            <span className={`text-[10px] ${
              i < currentStep ? 'text-green-400' :
              i === currentStep ? 'text-white' : 'text-zinc-500'
            }`}>{step}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
