import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { ArrowLeft, Loader2, Wifi, Server } from 'lucide-react';
import { useStore, PiInfo } from '../../lib/store';

interface WaitingPiProps {
  onPiFound: (info: PiInfo) => void;
  onBack: () => void;
}

export default function WaitingPi({ onPiFound, onBack }: WaitingPiProps) {
  const { config, addLog } = useStore();
  const [searching, setSearching] = useState(true);
  const [attempts, setAttempts] = useState(0);
  const maxAttempts = 60; // 5 minutes

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
          setSearching(false);
          onPiFound(piInfo);
        } else {
          setAttempts((prev) => prev + 1);
        }
      } catch (error) {
        console.error('Erreur recherche Pi:', error);
        setAttempts((prev) => prev + 1);
      }
    }, 5000);

    return () => clearInterval(searchInterval);
  }, [config.hostname, attempts]);

  // Timeout après 5 minutes
  useEffect(() => {
    if (attempts >= maxAttempts) {
      setSearching(false);
    }
  }, [attempts]);

  return (
    <div className="space-y-8">
      <div className="text-center">
        <div className="w-24 h-24 mx-auto bg-green-500/10 rounded-3xl flex items-center justify-center mb-6">
          <span className="text-5xl">🔌</span>
        </div>
        <h2 className="text-2xl font-bold text-white mb-2">
          Carte SD prête !
        </h2>
        <p className="text-gray-400">
          Suivez les instructions ci-dessous
        </p>
      </div>

      {/* Instructions */}
      <div className="bg-gray-800/50 rounded-xl p-6 space-y-4">
        <h3 className="font-medium text-white mb-4">Instructions :</h3>

        <div className="flex items-start gap-4">
          <div className="w-8 h-8 bg-purple-500/20 rounded-full flex items-center justify-center flex-shrink-0">
            <span className="text-purple-400 font-bold">1</span>
          </div>
          <div>
            <p className="text-white font-medium">Retirez la carte SD</p>
            <p className="text-sm text-gray-400">
              de votre ordinateur en toute sécurité
            </p>
          </div>
        </div>

        <div className="flex items-start gap-4">
          <div className="w-8 h-8 bg-purple-500/20 rounded-full flex items-center justify-center flex-shrink-0">
            <span className="text-purple-400 font-bold">2</span>
          </div>
          <div>
            <p className="text-white font-medium">Insérez-la dans le Raspberry Pi</p>
            <p className="text-sm text-gray-400">
              Le slot se trouve sous le Pi
            </p>
          </div>
        </div>

        <div className="flex items-start gap-4">
          <div className="w-8 h-8 bg-purple-500/20 rounded-full flex items-center justify-center flex-shrink-0">
            <span className="text-purple-400 font-bold">3</span>
          </div>
          <div>
            <p className="text-white font-medium">Branchez l'alimentation</p>
            <p className="text-sm text-gray-400">
              Le Pi va démarrer automatiquement
            </p>
          </div>
        </div>

        <div className="flex items-start gap-4">
          <div className="w-8 h-8 bg-purple-500/20 rounded-full flex items-center justify-center flex-shrink-0">
            <span className="text-purple-400 font-bold">4</span>
          </div>
          <div>
            <p className="text-white font-medium">Patientez 2-3 minutes</p>
            <p className="text-sm text-gray-400">
              Le temps que le système démarre
            </p>
          </div>
        </div>
      </div>

      {/* Status */}
      <div className="bg-gray-800/30 rounded-xl p-6">
        {searching ? (
          <div className="flex items-center gap-4">
            <div className="w-12 h-12 bg-blue-500/10 rounded-xl flex items-center justify-center">
              <Loader2 className="w-6 h-6 text-blue-400 animate-spin" />
            </div>
            <div className="flex-1">
              <p className="text-white font-medium">
                Recherche du Raspberry Pi sur le réseau...
              </p>
              <p className="text-sm text-gray-400">
                Tentative {attempts} / {maxAttempts}
              </p>
            </div>
            <div className="flex gap-1">
              {[0, 1, 2].map((i) => (
                <div
                  key={i}
                  className="w-2 h-2 bg-blue-400 rounded-full animate-pulse"
                  style={{ animationDelay: `${i * 0.2}s` }}
                />
              ))}
            </div>
          </div>
        ) : attempts >= maxAttempts ? (
          <div className="text-center space-y-4">
            <div className="w-16 h-16 mx-auto bg-orange-500/10 rounded-xl flex items-center justify-center">
              <Wifi className="w-8 h-8 text-orange-400" />
            </div>
            <div>
              <p className="text-white font-medium mb-1">Pi non trouvé</p>
              <p className="text-sm text-gray-400">
                Vérifiez que le Pi est allumé et connecté au même réseau WiFi
              </p>
            </div>
            <button
              onClick={() => setAttempts(0)}
              className="px-6 py-2 bg-purple-600 hover:bg-purple-700 text-white rounded-lg transition-colors"
            >
              Réessayer
            </button>
          </div>
        ) : null}
      </div>

      {/* Tips */}
      <div className="bg-blue-500/10 border border-blue-500/30 rounded-xl p-4">
        <div className="flex items-start gap-3">
          <Server className="w-5 h-5 text-blue-400 flex-shrink-0 mt-0.5" />
          <div>
            <p className="text-sm text-blue-300">
              <strong>Conseil :</strong> Le Raspberry Pi doit être connecté au même
              réseau WiFi que cet ordinateur pour être détecté.
            </p>
          </div>
        </div>
      </div>

      {/* Back button */}
      <div className="flex justify-start">
        <button
          onClick={onBack}
          className="inline-flex items-center gap-2 px-6 py-3 text-gray-400 hover:text-white transition-colors"
        >
          <ArrowLeft className="w-5 h-5" />
          Retour
        </button>
      </div>
    </div>
  );
}
