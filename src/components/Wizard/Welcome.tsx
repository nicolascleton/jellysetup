import { ArrowRight, Play } from 'lucide-react';

interface WelcomeProps {
  onNext: () => void;
}

export default function Welcome({ onNext }: WelcomeProps) {
  return (
    <div className="flex flex-col items-center justify-center min-h-[400px] text-center">
      {/* Logo */}
      <div className="w-24 h-24 bg-gradient-to-br from-red-500 via-pink-500 to-red-600 rounded-3xl flex items-center justify-center shadow-2xl shadow-red-500/30 mb-8 animate-float">
        <span className="text-5xl">🍓</span>
      </div>

      {/* Title */}
      <h1 className="text-3xl font-bold text-white mb-3">
        JellySetup
      </h1>
      <p className="text-zinc-400 mb-8 max-w-sm">
        Configurez votre Media Center Raspberry Pi en quelques clics
      </p>

      {/* CTA Button */}
      <button
        onClick={onNext}
        className="btn-primary text-lg px-10 py-4 group"
      >
        <Play className="w-5 h-5" />
        Démarrer
        <ArrowRight className="w-5 h-5 group-hover:translate-x-1 transition-transform" />
      </button>
    </div>
  );
}
