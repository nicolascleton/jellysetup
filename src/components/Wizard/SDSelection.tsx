import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { ArrowLeft, ArrowRight, RefreshCw, HardDrive, Check } from 'lucide-react';
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
    return `${gb.toFixed(0)} GB`;
  };

  const handleNext = () => {
    if (selectedSD && confirmed) onNext();
  };

  return (
    <div className="space-y-4">
      {/* SD Cards List */}
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm text-zinc-400">Cartes détectées</span>
        <button onClick={loadSDCards} disabled={loading} className="btn-ghost text-xs py-1">
          <RefreshCw className={`w-3 h-3 ${loading ? 'animate-spin' : ''}`} />
          Actualiser
        </button>
      </div>

      <div className="space-y-2 max-h-[200px] overflow-auto">
        {loading ? (
          <div className="card !p-8 text-center">
            <RefreshCw className="w-6 h-6 text-zinc-600 animate-spin mx-auto mb-2" />
            <p className="text-sm text-zinc-500">Recherche...</p>
          </div>
        ) : sdCards.length === 0 ? (
          <div className="card !p-8 text-center">
            <HardDrive className="w-6 h-6 text-zinc-600 mx-auto mb-2" />
            <p className="text-sm text-zinc-400">Aucune carte SD</p>
          </div>
        ) : (
          sdCards.map((card) => {
            const isSelected = selectedSD?.path === card.path;
            return (
              <button
                key={card.path}
                onClick={() => { setSelectedSD(card); setConfirmed(false); }}
                className={`w-full p-3 rounded-xl border-2 transition-all text-left ${
                  isSelected
                    ? 'bg-purple-500/10 border-purple-500'
                    : 'card border-transparent hover:border-zinc-700'
                }`}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <HardDrive className={`w-5 h-5 ${isSelected ? 'text-purple-400' : 'text-zinc-400'}`} />
                    <div>
                      <p className="font-medium text-white text-sm">{card.name}</p>
                      <p className="text-xs text-zinc-500">{formatSize(card.size)}</p>
                    </div>
                  </div>
                  {isSelected && <Check className="w-5 h-5 text-purple-400" />}
                </div>
              </button>
            );
          })
        )}
      </div>

      {/* Confirmation */}
      {selectedSD && (
        <label className="flex items-center gap-3 p-3 card cursor-pointer text-sm">
          <input
            type="checkbox"
            checked={confirmed}
            onChange={(e) => setConfirmed(e.target.checked)}
            className="w-4 h-4 rounded border-zinc-600 bg-zinc-800 text-purple-500"
          />
          <span className="text-zinc-300">
            Effacer <span className="font-semibold text-white">{selectedSD.name}</span>
          </span>
        </label>
      )}

      {/* Navigation */}
      <div className="flex justify-between pt-2">
        <button onClick={onBack} className="btn-ghost">
          <ArrowLeft className="w-4 h-4" />
          Retour
        </button>
        <button onClick={handleNext} disabled={!selectedSD || !confirmed} className="btn-primary">
          Configurer
          <ArrowRight className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}
