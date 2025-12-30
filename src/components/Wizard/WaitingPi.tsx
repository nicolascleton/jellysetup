import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { ArrowLeft, Loader2, Wifi, RefreshCw, CheckCircle2, Edit3, Search } from 'lucide-react';
import { useStore, PiInfo } from '../../lib/store';

interface WaitingPiProps {
  onPiFound: (info: PiInfo) => void;
  onBack: () => void;
}

export default function WaitingPi({ onPiFound, onBack }: WaitingPiProps) {
  const { config, addLog, setConfig } = useStore();
  const [attempts, setAttempts] = useState(0);
  const [manualMode, setManualMode] = useState(false);
  const [manualInput, setManualInput] = useState('');
  const [inputError, setInputError] = useState('');
  const maxAttempts = 60;

  // Utiliser des refs pour éviter les re-renders infinis
  const onPiFoundRef = useRef(onPiFound);
  const addLogRef = useRef(addLog);
  onPiFoundRef.current = onPiFound;
  addLogRef.current = addLog;

  useEffect(() => {
    if (manualMode) return;

    let cancelled = false;
    let currentAttempt = 0;

    const doSearch = async () => {
      if (cancelled) return;

      currentAttempt++;
      const hostname = config.hostname || 'jellypi';
      console.log(`[WaitingPi] Starting search for ${hostname}.local (attempt ${currentAttempt})...`);
      addLogRef.current(`Recherche de ${hostname}.local (tentative ${currentAttempt})...`);
      setAttempts(currentAttempt);

      try {
        console.log('[WaitingPi] Calling discover_pi...');
        const piInfo = await invoke<PiInfo | null>('discover_pi', {
          hostname: hostname,
          timeout_secs: 10,
        });
        console.log('[WaitingPi] discover_pi returned:', piInfo);

        if (cancelled) return;

        if (piInfo) {
          addLogRef.current(`Pi trouvé: ${piInfo.ip}`);
          onPiFoundRef.current(piInfo);
        } else {
          addLogRef.current(`Pi non trouvé, nouvelle tentative dans 8s...`);
        }
      } catch (error) {
        console.error('[WaitingPi] discover_pi error:', error);
        addLogRef.current(`Erreur de recherche: ${error}`);
      }
    };

    // Lancer la recherche immédiatement puis toutes les 8 secondes
    doSearch();
    const searchInterval = setInterval(doSearch, 8000);

    return () => {
      cancelled = true;
      clearInterval(searchInterval);
    };
  }, [config.hostname, manualMode]);

  const handleManualConnect = async () => {
    setInputError('');
    const input = manualInput.trim();

    if (!input) {
      setInputError('Entrez une IP ou un hostname');
      return;
    }

    // Déterminer si c'est une IP ou un hostname
    const isIP = /^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(input);
    const hostname = isIP ? input : input.replace('.local', '');

    addLog(`Connexion manuelle à ${input}...`);

    try {
      if (isIP) {
        // Connexion directe avec l'IP
        addLog(`Pi trouvé à ${input}`);
        onPiFound({
          ip: input,
          hostname: hostname,
          macAddress: undefined,
        });
      } else {
        // Essayer de résoudre le hostname via mDNS
        const piInfo = await invoke<PiInfo | null>('discover_pi', {
          hostname: hostname,
          timeout_secs: 10,
        });

        if (piInfo) {
          // Mettre à jour le hostname dans la config si différent
          if (hostname !== config.hostname) {
            setConfig({ hostname });
          }
          addLog(`Pi trouvé: ${piInfo.ip}`);
          onPiFound(piInfo);
        } else {
          setInputError(`Impossible de trouver ${hostname}.local`);
        }
      }
    } catch (error) {
      setInputError(`Erreur: ${error}`);
    }
  };

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

      {!manualMode ? (
        <>
          {/* Status recherche auto */}
          <div className="card !p-4 flex items-center gap-4">
            <Loader2 className="w-6 h-6 text-blue-400 animate-spin flex-shrink-0" />
            <div className="flex-1">
              <p className="text-sm text-white">Recherche de <span className="font-mono text-blue-400">{config.hostname}.local</span>...</p>
              <div className="flex items-center gap-2 mt-1">
                <div className="flex-1 h-1 bg-zinc-800 rounded-full">
                  <div className="h-full bg-blue-500 rounded-full" style={{ width: `${(attempts / maxAttempts) * 100}%` }} />
                </div>
                <span className="text-xs text-zinc-500">{attempts}/{maxAttempts}</span>
              </div>
            </div>
          </div>

          {/* Bouton mode manuel */}
          <button
            onClick={() => setManualMode(true)}
            className="w-full card !p-3 flex items-center justify-center gap-2 text-sm text-zinc-400 hover:text-white hover:bg-zinc-800/50 transition-colors"
          >
            <Edit3 className="w-4 h-4" />
            Connexion manuelle (IP ou hostname)
          </button>
        </>
      ) : (
        <>
          {/* Mode manuel */}
          <div className="card !p-4 space-y-3">
            <p className="text-sm text-zinc-400">
              Entrez l'IP ou le hostname de votre Pi:
            </p>
            <div className="flex gap-2">
              <input
                type="text"
                value={manualInput}
                onChange={(e) => setManualInput(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleManualConnect()}
                placeholder="192.168.1.x ou maison"
                className={`input-field flex-1 text-sm py-2.5 font-mono ${inputError ? 'input-field-error' : ''}`}
                autoFocus
              />
              <button onClick={handleManualConnect} className="btn-primary !px-4">
                <Search className="w-4 h-4" />
              </button>
            </div>
            {inputError && <p className="text-xs text-red-400">{inputError}</p>}
            <p className="text-xs text-zinc-500">
              Astuce: utilisez <span className="font-mono">ping maison.local</span> dans le terminal pour trouver l'IP
            </p>
          </div>

          {/* Bouton retour recherche auto */}
          <button
            onClick={() => { setManualMode(false); setAttempts(0); }}
            className="w-full card !p-3 flex items-center justify-center gap-2 text-sm text-zinc-400 hover:text-white hover:bg-zinc-800/50 transition-colors"
          >
            <RefreshCw className="w-4 h-4" />
            Reprendre la recherche automatique
          </button>
        </>
      )}

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
        {!manualMode && attempts >= maxAttempts && (
          <button onClick={() => setAttempts(0)} className="btn-primary">
            <RefreshCw className="w-4 h-4" />
            Réessayer
          </button>
        )}
      </div>
    </div>
  );
}
