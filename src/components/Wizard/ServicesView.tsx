import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { Monitor, ArrowLeft, Loader2, Wifi, CheckCircle2 } from 'lucide-react';
import Complete from './Complete';
import { PiInfo } from '../../lib/store';

interface ServicesViewProps {
  onBack: () => void;
}

export default function ServicesView({ onBack }: ServicesViewProps) {
  const [piInfo, setPiInfo] = useState<PiInfo | null>(null);
  const [searching, setSearching] = useState(true);
  const [attempts, setAttempts] = useState(0);
  const [foundPis, setFoundPis] = useState<PiInfo[]>([]);

  // Scanner le réseau automatiquement
  useEffect(() => {
    let cancelled = false;
    let currentAttempt = 0;

    const scanNetwork = async () => {
      if (cancelled) return;

      currentAttempt++;
      setAttempts(currentAttempt);

      try {
        // Scanner avec différents hostnames courants
        const hostnames = ['jellypi', 'raspberrypi', 'jellypi-nico'];

        for (const hostname of hostnames) {
          if (cancelled) return;

          const result = await invoke<PiInfo | null>('discover_pi', {
            hostname: hostname,
            timeoutSecs: 5,
          });

          if (result && !cancelled) {
            // Vérifier que Jellyfin tourne sur ce Pi (port 8096)
            setFoundPis(prev => {
              const exists = prev.some(p => p.ip === result.ip);
              if (!exists) {
                return [...prev, { ...result, hostname }];
              }
              return prev;
            });
          }
        }
      } catch (error) {
        console.error('Scan error:', error);
      }

      // Continuer à scanner si aucun Pi trouvé
      if (!cancelled && foundPis.length === 0 && currentAttempt < 10) {
        setTimeout(scanNetwork, 3000);
      } else {
        setSearching(false);
      }
    };

    scanNetwork();

    return () => {
      cancelled = true;
    };
  }, []);

  // Si on a sélectionné un Pi, afficher Complete
  if (piInfo) {
    return (
      <Complete
        piInfo={piInfo}
        onRestart={() => setPiInfo(null)}
      />
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="text-center">
        <div className="w-20 h-20 mx-auto bg-gradient-to-br from-orange-500 to-amber-500 rounded-3xl flex items-center justify-center mb-4 shadow-xl shadow-orange-500/30">
          {searching ? (
            <Loader2 className="w-10 h-10 text-white animate-spin" />
          ) : (
            <Monitor className="w-10 h-10 text-white" />
          )}
        </div>
        <h2 className="text-2xl font-bold text-white mb-2">
          {searching ? 'Recherche en cours...' : 'Raspberry Pi trouvés'}
        </h2>
        <p className="text-zinc-400 text-sm">
          {searching
            ? `Scan du réseau local (tentative ${attempts})...`
            : foundPis.length > 0
              ? 'Sélectionnez votre media center'
              : 'Aucun Pi détecté sur le réseau'
          }
        </p>
      </div>

      {/* Liste des Pi trouvés */}
      {foundPis.length > 0 && (
        <div className="space-y-3">
          {foundPis.map((pi) => (
            <button
              key={pi.ip}
              onClick={() => setPiInfo(pi)}
              className="w-full card !p-4 group hover:border-orange-500/50 hover:bg-orange-500/5 transition-all duration-300 text-left"
            >
              <div className="flex items-center gap-4">
                <div className="w-12 h-12 bg-gradient-to-br from-green-500 to-emerald-500 rounded-xl flex items-center justify-center flex-shrink-0">
                  <CheckCircle2 className="w-6 h-6 text-white" />
                </div>
                <div className="flex-1">
                  <h3 className="font-semibold text-white">{pi.hostname}.local</h3>
                  <p className="text-sm text-zinc-400 font-mono">{pi.ip}</p>
                </div>
                <Wifi className="w-5 h-5 text-green-400" />
              </div>
            </button>
          ))}
        </div>
      )}

      {/* Animation de recherche */}
      {searching && foundPis.length === 0 && (
        <div className="card !p-6">
          <div className="flex items-center gap-4">
            <div className="relative">
              <Wifi className="w-8 h-8 text-orange-400" />
              <div className="absolute inset-0 animate-ping">
                <Wifi className="w-8 h-8 text-orange-400 opacity-50" />
              </div>
            </div>
            <div>
              <p className="text-white font-medium">Scan du réseau...</p>
              <p className="text-sm text-zinc-500">Recherche de Raspberry Pi avec Jellyfin</p>
            </div>
          </div>
        </div>
      )}

      {/* Message si aucun Pi trouvé */}
      {!searching && foundPis.length === 0 && (
        <div className="card !p-6 border-yellow-500/30 bg-yellow-500/5">
          <p className="text-yellow-400 text-sm">
            Aucun Raspberry Pi détecté. Vérifiez que votre Pi est allumé et connecté au même réseau WiFi.
          </p>
        </div>
      )}

      {/* Actions */}
      <div className="flex gap-3">
        <button
          onClick={onBack}
          className="btn-secondary flex items-center gap-2"
        >
          <ArrowLeft className="w-4 h-4" />
          Retour
        </button>
        {!searching && (
          <button
            onClick={() => {
              setSearching(true);
              setAttempts(0);
              setFoundPis([]);
            }}
            className="btn-primary flex-1"
          >
            Relancer la recherche
          </button>
        )}
      </div>
    </div>
  );
}
