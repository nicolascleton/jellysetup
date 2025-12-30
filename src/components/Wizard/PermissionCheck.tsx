import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { dialog } from '@tauri-apps/api';
import { Shield, CheckCircle, AlertTriangle, ExternalLink } from 'lucide-react';

interface PermissionCheckProps {
  onGranted: () => void;
}

const FDA_SKIP_KEY = 'jellysetup_fda_confirmed';

export default function PermissionCheck({ onGranted }: PermissionCheckProps) {
  const [checking, setChecking] = useState(true);
  const [hasAccess, setHasAccess] = useState(false);

  useEffect(() => {
    checkPermission();
  }, []);

  const checkPermission = async () => {
    setChecking(true);

    // Si l'utilisateur a déjà confirmé avoir activé FDA, on skip le check
    // (évite la boucle infinie en mode dev où Terminal a besoin du FDA)
    const fdaConfirmed = localStorage.getItem(FDA_SKIP_KEY);
    if (fdaConfirmed === 'true') {
      console.log('FDA already confirmed by user, skipping check');
      setHasAccess(true);
      setTimeout(onGranted, 500);
      setChecking(false);
      return;
    }

    try {
      const result = await invoke<boolean>('check_disk_access');
      setHasAccess(result);
      if (result) {
        setTimeout(onGranted, 500);
      }
    } catch (e) {
      console.error('Permission check error:', e);
      setHasAccess(false);
    }
    setChecking(false);
  };

  const openSettingsAndWaitForRestart = async () => {
    try {
      // Ouvrir les réglages
      await invoke('open_disk_access_settings');

      // Attendre 3 secondes puis afficher le popup
      setTimeout(async () => {
        const confirmed = await dialog.confirm(
          'As-tu activé JellySetup dans les réglages ?',
          {
            title: 'Redémarrage requis',
            okLabel: 'Relancer',
            cancelLabel: 'Pas encore'
          }
        );

        if (confirmed) {
          // Sauvegarder le flag pour éviter la boucle infinie au prochain démarrage
          localStorage.setItem(FDA_SKIP_KEY, 'true');
          // Relancer l'app
          await invoke('restart_app');
        }
      }, 3000);
    } catch (e) {
      console.error('Failed to open settings:', e);
    }
  };

  if (checking) {
    return (
      <div className="text-center space-y-4">
        <div className="w-16 h-16 mx-auto bg-purple-500/20 rounded-2xl flex items-center justify-center animate-pulse">
          <Shield className="w-8 h-8 text-purple-400" />
        </div>
        <p className="text-zinc-400">Vérification des permissions...</p>
      </div>
    );
  }

  if (hasAccess) {
    return (
      <div className="text-center space-y-4">
        <div className="w-16 h-16 mx-auto bg-green-500/20 rounded-2xl flex items-center justify-center">
          <CheckCircle className="w-8 h-8 text-green-400" />
        </div>
        <div>
          <h3 className="text-lg font-semibold text-white mb-1">Permissions OK</h3>
          <p className="text-sm text-zinc-400">JellySetup peut écrire sur la carte SD</p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="text-center">
        <div className="w-16 h-16 mx-auto bg-orange-500/20 rounded-2xl flex items-center justify-center mb-4">
          <AlertTriangle className="w-8 h-8 text-orange-400" />
        </div>
        <h2 className="text-2xl font-bold text-white mb-2">Permission requise</h2>
        <p className="text-zinc-400">
          macOS exige l'Accès complet au disque pour flasher la carte SD
        </p>
      </div>

      <div className="glass-card p-6 space-y-4">
        <div className="space-y-3">
          <div className="flex items-start gap-3">
            <span className="w-6 h-6 bg-purple-500/20 rounded-full flex items-center justify-center text-sm text-purple-400 font-medium flex-shrink-0">1</span>
            <p className="text-sm text-zinc-300">Clique sur <strong>"Ouvrir les Réglages"</strong></p>
          </div>

          <div className="flex items-start gap-3">
            <span className="w-6 h-6 bg-purple-500/20 rounded-full flex items-center justify-center text-sm text-purple-400 font-medium flex-shrink-0">2</span>
            <p className="text-sm text-zinc-300">Active <strong>JellySetup</strong> dans la liste</p>
          </div>

          <div className="flex items-start gap-3">
            <span className="w-6 h-6 bg-purple-500/20 rounded-full flex items-center justify-center text-sm text-purple-400 font-medium flex-shrink-0">3</span>
            <p className="text-sm text-zinc-300">Un popup apparaîtra pour relancer l'app</p>
          </div>
        </div>
      </div>

      <button
        onClick={openSettingsAndWaitForRestart}
        className="btn-primary w-full flex items-center justify-center gap-2"
      >
        <ExternalLink className="w-4 h-4" />
        Ouvrir les Réglages
      </button>
    </div>
  );
}
