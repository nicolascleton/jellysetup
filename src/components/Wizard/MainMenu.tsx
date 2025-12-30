import { HardDrive, Search, Settings, ArrowRight, Sparkles } from 'lucide-react';

interface MainMenuProps {
  onNewSetup: () => void;
  onConnectExisting: () => void;
  onReconfigure: () => void;
  hasExistingConfig: boolean;
}

export default function MainMenu({
  onNewSetup,
  onConnectExisting,
  onReconfigure,
  hasExistingConfig
}: MainMenuProps) {
  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="text-center">
        <div className="w-20 h-20 mx-auto bg-gradient-to-br from-purple-500 to-pink-500 rounded-3xl flex items-center justify-center mb-4 shadow-xl shadow-purple-500/30">
          <span className="text-4xl">🍓</span>
        </div>
        <h2 className="text-2xl font-bold text-white mb-2">
          Bienvenue sur JellySetup
        </h2>
        <p className="text-zinc-400 text-sm">
          Configurez votre media center Raspberry Pi en quelques clics
        </p>
      </div>

      {/* Options */}
      <div className="space-y-3">
        {/* Option 1: Nouveau setup complet */}
        <button
          onClick={onNewSetup}
          className="w-full card !p-5 group hover:border-purple-500/50 hover:bg-purple-500/5 transition-all duration-300 text-left"
        >
          <div className="flex items-start gap-4">
            <div className="w-12 h-12 bg-gradient-to-br from-purple-500 to-pink-500 rounded-xl flex items-center justify-center flex-shrink-0 group-hover:scale-110 transition-transform">
              <HardDrive className="w-6 h-6 text-white" />
            </div>
            <div className="flex-1">
              <h3 className="font-semibold text-white flex items-center gap-2">
                Nouveau setup complet
                <Sparkles className="w-4 h-4 text-purple-400" />
              </h3>
              <p className="text-sm text-zinc-400 mt-1">
                Flash de la carte SD + installation de tous les services
              </p>
              <div className="flex flex-wrap gap-2 mt-2">
                {['Jellyfin', 'Radarr', 'Sonarr', 'Prowlarr'].map((svc) => (
                  <span key={svc} className="text-xs px-2 py-0.5 bg-zinc-800 rounded-full text-zinc-400">
                    {svc}
                  </span>
                ))}
              </div>
            </div>
            <ArrowRight className="w-5 h-5 text-zinc-600 group-hover:text-purple-400 group-hover:translate-x-1 transition-all" />
          </div>
        </button>

        {/* Option 2: Connecter un Pi existant */}
        <button
          onClick={onConnectExisting}
          className="w-full card !p-5 group hover:border-blue-500/50 hover:bg-blue-500/5 transition-all duration-300 text-left"
        >
          <div className="flex items-start gap-4">
            <div className="w-12 h-12 bg-gradient-to-br from-blue-500 to-cyan-500 rounded-xl flex items-center justify-center flex-shrink-0 group-hover:scale-110 transition-transform">
              <Search className="w-6 h-6 text-white" />
            </div>
            <div className="flex-1">
              <h3 className="font-semibold text-white">
                Connecter un Pi existant
              </h3>
              <p className="text-sm text-zinc-400 mt-1">
                J'ai déjà flashé la carte SD, je veux juste lancer l'installation
              </p>
              {hasExistingConfig && (
                <div className="mt-2 text-xs text-blue-400">
                  Session précédente détectée
                </div>
              )}
            </div>
            <ArrowRight className="w-5 h-5 text-zinc-600 group-hover:text-blue-400 group-hover:translate-x-1 transition-all" />
          </div>
        </button>

        {/* Option 3: Reconfigurer */}
        <button
          onClick={onReconfigure}
          className="w-full card !p-5 group hover:border-green-500/50 hover:bg-green-500/5 transition-all duration-300 text-left"
        >
          <div className="flex items-start gap-4">
            <div className="w-12 h-12 bg-gradient-to-br from-green-500 to-emerald-500 rounded-xl flex items-center justify-center flex-shrink-0 group-hover:scale-110 transition-transform">
              <Settings className="w-6 h-6 text-white" />
            </div>
            <div className="flex-1">
              <h3 className="font-semibold text-white">
                Reconfigurer un Pi
              </h3>
              <p className="text-sm text-zinc-400 mt-1">
                Modifier les clés API ou ajouter des services
              </p>
            </div>
            <ArrowRight className="w-5 h-5 text-zinc-600 group-hover:text-green-400 group-hover:translate-x-1 transition-all" />
          </div>
        </button>
      </div>

      {/* Info */}
      <div className="text-center text-xs text-zinc-600">
        Tous les services sont installés automatiquement sur votre Raspberry Pi
      </div>
    </div>
  );
}
