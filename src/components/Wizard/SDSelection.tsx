import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { ArrowLeft, ArrowRight, RefreshCw, AlertTriangle, HardDrive, Check } from 'lucide-react';
import { useStore, SDCard } from '../../lib/store';

interface SDSelectionProps {
  onNext: () => void;
  onBack: () => void;
}

export default function SDSelection({ onNext, onBack }: SDSelectionProps) {
  const [sdCards, setSDCards] = useState<SDCard[]>([]);
  const [loading, setLoading] = useState(true);
  const [confirmed, setConfirmed] = useState(false);
  const { selectedSD, setSelectedSD } = useStore();

  const loadSDCards = async () => {
    setLoading(true);
    try {
      const cards = await invoke<SDCard[]>('list_sd_cards');
      setSDCards(cards);
    } catch (error) {
      console.error('Erreur détection SD:', error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadSDCards();
  }, []);

  const formatSize = (bytes: number): string => {
    const gb = bytes / (1024 * 1024 * 1024);
    return `${gb.toFixed(1)} GB`;
  };

  const handleNext = () => {
    if (selectedSD && confirmed) {
      onNext();
    }
  };

  return (
    <div className="space-y-6">
      <div className="text-center mb-8">
        <h2 className="text-2xl font-bold text-white mb-2">
          Sélection de la carte SD
        </h2>
        <p className="text-gray-400">
          Choisissez la carte SD sur laquelle installer le système
        </p>
      </div>

      {/* Warning */}
      <div className="bg-orange-500/10 border border-orange-500/30 rounded-xl p-4 flex items-start gap-3">
        <AlertTriangle className="w-6 h-6 text-orange-400 flex-shrink-0 mt-0.5" />
        <div>
          <h4 className="font-medium text-orange-400 mb-1">Attention</h4>
          <p className="text-sm text-orange-300/80">
            Toutes les données présentes sur la carte SD seront effacées.
            Assurez-vous d'avoir sauvegardé vos données importantes.
          </p>
        </div>
      </div>

      {/* SD Cards List */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-medium text-gray-400">
            Cartes SD détectées
          </h3>
          <button
            onClick={loadSDCards}
            disabled={loading}
            className="inline-flex items-center gap-2 px-3 py-1.5 text-sm text-gray-400 hover:text-white transition-colors disabled:opacity-50"
          >
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
            Actualiser
          </button>
        </div>

        {loading ? (
          <div className="bg-gray-800/50 rounded-xl p-8 text-center">
            <RefreshCw className="w-8 h-8 text-gray-500 animate-spin mx-auto mb-3" />
            <p className="text-gray-400">Recherche des cartes SD...</p>
          </div>
        ) : sdCards.length === 0 ? (
          <div className="bg-gray-800/50 rounded-xl p-8 text-center">
            <HardDrive className="w-12 h-12 text-gray-600 mx-auto mb-3" />
            <p className="text-gray-400 mb-2">Aucune carte SD détectée</p>
            <p className="text-sm text-gray-500">
              Insérez une carte SD et cliquez sur "Actualiser"
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {sdCards.map((card) => (
              <button
                key={card.path}
                onClick={() => {
                  setSelectedSD(card);
                  setConfirmed(false);
                }}
                className={`w-full p-4 rounded-xl border-2 transition-all text-left ${
                  selectedSD?.path === card.path
                    ? 'bg-purple-500/10 border-purple-500'
                    : 'bg-gray-800/50 border-gray-700 hover:border-gray-600'
                }`}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <div
                      className={`w-10 h-10 rounded-lg flex items-center justify-center ${
                        selectedSD?.path === card.path
                          ? 'bg-purple-500/20'
                          : 'bg-gray-700'
                      }`}
                    >
                      <HardDrive
                        className={`w-5 h-5 ${
                          selectedSD?.path === card.path
                            ? 'text-purple-400'
                            : 'text-gray-400'
                        }`}
                      />
                    </div>
                    <div>
                      <p className="font-medium text-white">{card.name}</p>
                      <p className="text-sm text-gray-400">
                        {card.path} • {formatSize(card.size)}
                      </p>
                    </div>
                  </div>
                  {selectedSD?.path === card.path && (
                    <div className="w-6 h-6 bg-purple-500 rounded-full flex items-center justify-center">
                      <Check className="w-4 h-4 text-white" />
                    </div>
                  )}
                </div>
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Confirmation */}
      {selectedSD && (
        <label className="flex items-center gap-3 p-4 bg-gray-800/50 rounded-xl cursor-pointer">
          <input
            type="checkbox"
            checked={confirmed}
            onChange={(e) => setConfirmed(e.target.checked)}
            className="w-5 h-5 rounded border-gray-600 bg-gray-700 text-purple-500 focus:ring-purple-500 focus:ring-offset-0"
          />
          <span className="text-sm text-gray-300">
            Je confirme vouloir effacer <strong className="text-white">{selectedSD.name}</strong> et
            installer Raspberry Pi OS dessus
          </span>
        </label>
      )}

      {/* Navigation */}
      <div className="flex justify-between pt-4">
        <button
          onClick={onBack}
          className="inline-flex items-center gap-2 px-6 py-3 text-gray-400 hover:text-white transition-colors"
        >
          <ArrowLeft className="w-5 h-5" />
          Retour
        </button>

        <button
          onClick={handleNext}
          disabled={!selectedSD || !confirmed}
          className="inline-flex items-center gap-2 px-8 py-3 bg-purple-600 hover:bg-purple-700 disabled:bg-gray-700 disabled:text-gray-500 text-white font-medium rounded-xl transition-colors"
        >
          Flasher la carte
          <ArrowRight className="w-5 h-5" />
        </button>
      </div>
    </div>
  );
}
