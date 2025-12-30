import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';

import Welcome from './components/Wizard/Welcome';
import ConfigForm from './components/Wizard/ConfigForm';
import SDSelection from './components/Wizard/SDSelection';
import FlashProgress from './components/Wizard/FlashProgress';
import WaitingPi from './components/Wizard/WaitingPi';
import ConfigProgress from './components/Wizard/ConfigProgress';
import Complete from './components/Wizard/Complete';

import { useStore } from './lib/store';

type WizardStep =
  | 'welcome'
  | 'config'
  | 'sd-selection'
  | 'flash'
  | 'waiting'
  | 'configure'
  | 'complete';

function App() {
  const [step, setStep] = useState<WizardStep>('welcome');
  const { config, setConfig, piInfo, setPiInfo } = useStore();

  // Écouter les mises à jour de version
  useEffect(() => {
    checkForUpdates();
  }, []);

  const checkForUpdates = async () => {
    try {
      const latestVersion = await invoke<string | null>('check_for_updates');
      if (latestVersion && latestVersion !== '1.0.0') {
        // Afficher notification de mise à jour
        console.log('Nouvelle version disponible:', latestVersion);
      }
    } catch (error) {
      console.error('Erreur vérification MAJ:', error);
    }
  };

  const renderStep = () => {
    switch (step) {
      case 'welcome':
        return <Welcome onNext={() => setStep('config')} />;

      case 'config':
        return (
          <ConfigForm
            config={config}
            onConfigChange={setConfig}
            onNext={() => setStep('sd-selection')}
            onBack={() => setStep('welcome')}
          />
        );

      case 'sd-selection':
        return (
          <SDSelection
            onNext={() => setStep('flash')}
            onBack={() => setStep('config')}
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
    <div className="min-h-screen bg-[#0f0f0f] flex flex-col">
      {/* Header */}
      <header className="px-6 py-4 border-b border-gray-800 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 bg-purple-600 rounded-xl flex items-center justify-center">
            <span className="text-xl">🍓</span>
          </div>
          <div>
            <h1 className="text-lg font-semibold text-white">JellySetup</h1>
            <p className="text-xs text-gray-400">v1.0.0</p>
          </div>
        </div>

        {/* Step indicator */}
        <div className="flex items-center gap-2">
          {['welcome', 'config', 'sd-selection', 'flash', 'waiting', 'configure', 'complete'].map((s, i) => (
            <div
              key={s}
              className={`w-2 h-2 rounded-full transition-colors ${
                step === s
                  ? 'bg-purple-500'
                  : ['welcome', 'config', 'sd-selection', 'flash', 'waiting', 'configure', 'complete'].indexOf(step) > i
                  ? 'bg-green-500'
                  : 'bg-gray-700'
              }`}
            />
          ))}
        </div>
      </header>

      {/* Content */}
      <main className="flex-1 flex items-center justify-center p-6">
        <div className="w-full max-w-2xl">
          {renderStep()}
        </div>
      </main>

      {/* Footer */}
      <footer className="px-6 py-3 border-t border-gray-800 text-center">
        <p className="text-xs text-gray-500">
          Besoin d'aide ? Contactez l'administrateur
        </p>
      </footer>
    </div>
  );
}

export default App;
