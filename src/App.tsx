import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { Check, Sparkles } from 'lucide-react';

import PermissionCheck from './components/Wizard/PermissionCheck';
import Welcome from './components/Wizard/Welcome';
import ConfigForm from './components/Wizard/ConfigForm';
import SDSelection from './components/Wizard/SDSelection';
import FlashProgress from './components/Wizard/FlashProgress';
import WaitingPi from './components/Wizard/WaitingPi';
import ConfigProgress from './components/Wizard/ConfigProgress';
import Complete from './components/Wizard/Complete';

import { useStore } from './lib/store';

type WizardStep =
  | 'permission'
  | 'welcome'
  | 'config'
  | 'sd-selection'
  | 'flash'
  | 'waiting'
  | 'configure'
  | 'complete';

// Étapes affichées dans le stepper (sans permission qui est une étape cachée)
const VISIBLE_STEPS = [
  { id: 'welcome', label: 'Accueil', shortLabel: '1' },
  { id: 'sd-selection', label: 'Carte SD', shortLabel: '2' },
  { id: 'config', label: 'Configuration', shortLabel: '3' },
  { id: 'flash', label: 'Installation', shortLabel: '4' },
  { id: 'waiting', label: 'Connexion', shortLabel: '5' },
  { id: 'configure', label: 'Setup', shortLabel: '6' },
  { id: 'complete', label: 'Terminé', shortLabel: '7' },
];


function App() {
  const [step, setStep] = useState<WizardStep>('permission');
  const { config, setConfig, piInfo, setPiInfo } = useStore();

  useEffect(() => {
    checkForUpdates();
  }, []);

  const checkForUpdates = async () => {
    try {
      const latestVersion = await invoke<string | null>('check_for_updates');
      if (latestVersion && latestVersion !== '1.0.0') {
        console.log('Nouvelle version disponible:', latestVersion);
      }
    } catch (error) {
      console.error('Erreur vérification MAJ:', error);
    }
  };

  // Pour l'affichage, on utilise VISIBLE_STEPS (exclut permission)
  const visibleStepIndex = VISIBLE_STEPS.findIndex((s) => s.id === step);
  // Si on est sur permission, on affiche comme si on était avant la première étape
  const currentStepIndex = step === 'permission' ? -1 : visibleStepIndex;

  const renderStep = () => {
    switch (step) {
      case 'permission':
        return <PermissionCheck onGranted={() => setStep('welcome')} />;
      case 'welcome':
        return <Welcome onNext={() => setStep('sd-selection')} />;
      case 'sd-selection':
        return (
          <SDSelection
            onNext={() => setStep('config')}
            onBack={() => setStep('welcome')}
          />
        );
      case 'config':
        return (
          <ConfigForm
            config={config}
            onConfigChange={setConfig}
            onNext={() => setStep('flash')}
            onBack={() => setStep('sd-selection')}
          />
        );
      case 'flash':
        return (
          <FlashProgress
            onComplete={() => setStep('waiting')}
            onError={() => setStep('sd-selection')}
          />
        );
      case 'waiting':
        return (
          <WaitingPi
            onPiFound={(info) => {
              setPiInfo(info);
              setStep('configure');
            }}
            onBack={() => setStep('sd-selection')}
          />
        );
      case 'configure':
        return (
          <ConfigProgress
            piInfo={piInfo!}
            onComplete={() => setStep('complete')}
            onError={() => setStep('waiting')}
          />
        );
      case 'complete':
        return (
          <Complete
            piInfo={piInfo!}
            onRestart={() => {
              setConfig({
                wifiSSID: '',
                wifiPassword: '',
                hostname: 'jellypi',
                alldebridKey: '',
                jellyfinUsername: '',
                jellyfinPassword: '',
              });
              setPiInfo(null);
              setStep('welcome');
            }}
          />
        );
      default:
        return <Welcome onNext={() => setStep('config')} />;
    }
  };

  return (
    <div className="min-h-screen flex flex-col bg-gradient-dark">
      {/* Decorative background elements */}
      <div className="fixed inset-0 overflow-hidden pointer-events-none">
        <div className="absolute -top-40 -right-40 w-80 h-80 bg-purple-500/20 rounded-full blur-3xl" />
        <div className="absolute -bottom-40 -left-40 w-80 h-80 bg-pink-500/20 rounded-full blur-3xl" />
      </div>

      {/* Header */}
      <header className="relative z-10 px-8 py-5 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <div className="relative">
            <div className="w-12 h-12 bg-gradient-primary rounded-2xl flex items-center justify-center shadow-lg shadow-purple-500/30">
              <span className="text-2xl">🍓</span>
            </div>
            <div className="absolute -top-1 -right-1 w-4 h-4 bg-green-500 rounded-full border-2 border-zinc-900" />
          </div>
          <div>
            <h1 className="text-xl font-bold text-white flex items-center gap-2">
              JellySetup
              <Sparkles className="w-4 h-4 text-purple-400" />
            </h1>
            <p className="text-xs text-zinc-500">Configuration automatique</p>
          </div>
        </div>

        {/* Step indicator - Modern horizontal stepper (caché pendant permission) */}
        {step !== 'permission' && (
          <div className="hidden md:flex items-center gap-1 bg-zinc-900/50 backdrop-blur-xl rounded-full px-2 py-2 border border-zinc-800">
            {VISIBLE_STEPS.map((s, i) => {
              const isComplete = currentStepIndex > i;
              const isActive = step === s.id;
              const isPending = currentStepIndex < i;

              return (
                <div key={s.id} className="flex items-center">
                  <div
                    className={`
                      w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium
                      transition-all duration-300
                      ${isActive ? 'bg-purple-500 text-white shadow-lg shadow-purple-500/50 scale-110' : ''}
                      ${isComplete ? 'bg-green-500/20 text-green-400' : ''}
                      ${isPending ? 'bg-zinc-800 text-zinc-500' : ''}
                    `}
                  >
                    {isComplete ? <Check className="w-4 h-4" /> : i + 1}
                  </div>
                  {i < VISIBLE_STEPS.length - 1 && (
                    <div
                      className={`w-4 h-0.5 mx-0.5 rounded transition-all duration-500 ${
                        isComplete ? 'bg-green-500' : 'bg-zinc-700'
                      }`}
                    />
                  )}
                </div>
              );
            })}
          </div>
        )}

        {/* Mobile step indicator (caché pendant permission) */}
        {step !== 'permission' && (
          <div className="md:hidden flex items-center gap-2 bg-zinc-900/50 backdrop-blur-xl rounded-full px-4 py-2 border border-zinc-800">
            <span className="text-sm font-medium text-white">
              {currentStepIndex + 1}
            </span>
            <span className="text-sm text-zinc-500">/</span>
            <span className="text-sm text-zinc-500">{VISIBLE_STEPS.length}</span>
          </div>
        )}
      </header>

      {/* Step label (caché pendant permission) */}
      {step !== 'permission' && (
        <div className="relative z-10 text-center py-2">
          <span className="text-sm text-zinc-500">
            Étape {currentStepIndex + 1}: {VISIBLE_STEPS[currentStepIndex]?.label}
          </span>
        </div>
      )}

      {/* Content */}
      <main className="relative z-10 flex-1 flex items-center justify-center px-6 py-4">
        <div className="w-full max-w-2xl animate-fade-in-up">
          {renderStep()}
        </div>
      </main>

      {/* Footer */}
      <footer className="relative z-10 px-8 py-4 text-center border-t border-zinc-800/50">
        <p className="text-xs text-zinc-600">
          JellySetup v1.0.0 • Besoin d'aide ?{' '}
          <a href="#" className="text-purple-400 hover:text-purple-300 transition-colors">
            Contactez l'administrateur
          </a>
        </p>
      </footer>
    </div>
  );
}

export default App;
