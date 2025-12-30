import { ArrowRight, Tv, Film, Radio, Download } from 'lucide-react';

interface WelcomeProps {
  onNext: () => void;
}

export default function Welcome({ onNext }: WelcomeProps) {
  return (
    <div className="text-center space-y-8">
      {/* Hero */}
      <div className="space-y-4">
        <div className="w-24 h-24 mx-auto bg-gradient-to-br from-purple-500 to-pink-500 rounded-3xl flex items-center justify-center shadow-lg shadow-purple-500/25">
          <span className="text-5xl">🍓</span>
        </div>
        <h1 className="text-3xl font-bold text-white">
          Bienvenue dans JellySetup
        </h1>
        <p className="text-lg text-gray-400 max-w-md mx-auto">
          Configurez votre Raspberry Pi Media Center en quelques minutes,
          sans aucune connaissance technique.
        </p>
      </div>

      {/* Features */}
      <div className="grid grid-cols-2 gap-4 max-w-lg mx-auto">
        <div className="bg-gray-800/50 rounded-xl p-4 text-left">
          <div className="w-10 h-10 bg-purple-500/20 rounded-lg flex items-center justify-center mb-3">
            <Film className="w-5 h-5 text-purple-400" />
          </div>
          <h3 className="font-medium text-white mb-1">Films & Séries</h3>
          <p className="text-sm text-gray-400">
            Accédez à vos médias préférés
          </p>
        </div>

        <div className="bg-gray-800/50 rounded-xl p-4 text-left">
          <div className="w-10 h-10 bg-blue-500/20 rounded-lg flex items-center justify-center mb-3">
            <Tv className="w-5 h-5 text-blue-400" />
          </div>
          <h3 className="font-medium text-white mb-1">Streaming</h3>
          <p className="text-sm text-gray-400">
            Regardez sur tous vos appareils
          </p>
        </div>

        <div className="bg-gray-800/50 rounded-xl p-4 text-left">
          <div className="w-10 h-10 bg-green-500/20 rounded-lg flex items-center justify-center mb-3">
            <Download className="w-5 h-5 text-green-400" />
          </div>
          <h3 className="font-medium text-white mb-1">Automatique</h3>
          <p className="text-sm text-gray-400">
            Téléchargement intelligent
          </p>
        </div>

        <div className="bg-gray-800/50 rounded-xl p-4 text-left">
          <div className="w-10 h-10 bg-orange-500/20 rounded-lg flex items-center justify-center mb-3">
            <Radio className="w-5 h-5 text-orange-400" />
          </div>
          <h3 className="font-medium text-white mb-1">Sous-titres</h3>
          <p className="text-sm text-gray-400">
            Français & English auto
          </p>
        </div>
      </div>

      {/* Requirements */}
      <div className="bg-gray-800/30 rounded-xl p-6 max-w-lg mx-auto text-left">
        <h3 className="font-medium text-white mb-3">Ce dont vous avez besoin :</h3>
        <ul className="space-y-2 text-sm text-gray-400">
          <li className="flex items-center gap-2">
            <span className="w-1.5 h-1.5 bg-green-500 rounded-full" />
            Un Raspberry Pi 5 (4GB ou 8GB)
          </li>
          <li className="flex items-center gap-2">
            <span className="w-1.5 h-1.5 bg-green-500 rounded-full" />
            Une carte SD de 32GB minimum
          </li>
          <li className="flex items-center gap-2">
            <span className="w-1.5 h-1.5 bg-green-500 rounded-full" />
            Un compte AllDebrid
          </li>
          <li className="flex items-center gap-2">
            <span className="w-1.5 h-1.5 bg-green-500 rounded-full" />
            Votre réseau WiFi
          </li>
        </ul>
      </div>

      {/* CTA */}
      <button
        onClick={onNext}
        className="inline-flex items-center gap-2 px-8 py-4 bg-purple-600 hover:bg-purple-700 text-white font-medium rounded-xl transition-colors shadow-lg shadow-purple-600/25"
      >
        Commencer l'installation
        <ArrowRight className="w-5 h-5" />
      </button>
    </div>
  );
}
