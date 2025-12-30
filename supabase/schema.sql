-- =============================================================================
-- JellySetup - Schéma Supabase
-- Exécuter ce script dans l'éditeur SQL de Supabase
-- =============================================================================

-- Table des installations
CREATE TABLE IF NOT EXISTS installations (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  created_at TIMESTAMPTZ DEFAULT NOW(),

  -- Identification
  pi_name VARCHAR(50) NOT NULL,
  pi_serial VARCHAR(16),

  -- Réseau
  local_ip INET,
  mac_address VARCHAR(17),

  -- SSH (la clé privée est chiffrée côté client)
  ssh_public_key TEXT NOT NULL,
  ssh_private_key_encrypted TEXT NOT NULL,
  ssh_user VARCHAR(50) DEFAULT 'maison',
  ssh_port INTEGER DEFAULT 22,

  -- Configuration
  alldebrid_configured BOOLEAN DEFAULT FALSE,
  ygg_configured BOOLEAN DEFAULT FALSE,
  cloudflare_configured BOOLEAN DEFAULT FALSE,

  -- Status
  status VARCHAR(20) DEFAULT 'pending',
  last_seen TIMESTAMPTZ,
  error_message TEXT,

  -- Métadonnées
  installer_version VARCHAR(20),
  os_version VARCHAR(50),
  installed_by VARCHAR(100)
);

-- Table des logs d'installation
CREATE TABLE IF NOT EXISTS installation_logs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  installation_id UUID REFERENCES installations(id) ON DELETE CASCADE,
  created_at TIMESTAMPTZ DEFAULT NOW(),
  step VARCHAR(50),
  status VARCHAR(20),
  message TEXT,
  duration_ms INTEGER
);

-- Index pour performances
CREATE INDEX IF NOT EXISTS idx_installations_status ON installations(status);
CREATE INDEX IF NOT EXISTS idx_installations_pi_name ON installations(pi_name);
CREATE INDEX IF NOT EXISTS idx_installations_created ON installations(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_logs_installation ON installation_logs(installation_id);
CREATE INDEX IF NOT EXISTS idx_logs_created ON installation_logs(created_at DESC);

-- =============================================================================
-- Row Level Security (RLS)
-- =============================================================================

ALTER TABLE installations ENABLE ROW LEVEL SECURITY;
ALTER TABLE installation_logs ENABLE ROW LEVEL SECURITY;

-- Politique: Insertion publique (pour les installations depuis l'app)
CREATE POLICY "Allow public insert" ON installations
  FOR INSERT WITH CHECK (true);

CREATE POLICY "Allow public insert logs" ON installation_logs
  FOR INSERT WITH CHECK (true);

-- Politique: Seul l'admin peut lire (MODIFIER L'EMAIL!)
CREATE POLICY "Admin can read all" ON installations
  FOR SELECT USING (
    auth.jwt() ->> 'email' = 'nicolascleton@gmail.com'
  );

CREATE POLICY "Admin can read logs" ON installation_logs
  FOR SELECT USING (
    auth.jwt() ->> 'email' = 'nicolascleton@gmail.com'
  );

-- Politique: Seul l'admin peut mettre à jour
CREATE POLICY "Admin can update" ON installations
  FOR UPDATE USING (
    auth.jwt() ->> 'email' = 'nicolascleton@gmail.com'
  );

-- Politique: Seul l'admin peut supprimer
CREATE POLICY "Admin can delete" ON installations
  FOR DELETE USING (
    auth.jwt() ->> 'email' = 'nicolascleton@gmail.com'
  );

CREATE POLICY "Admin can delete logs" ON installation_logs
  FOR DELETE USING (
    auth.jwt() ->> 'email' = 'nicolascleton@gmail.com'
  );

-- =============================================================================
-- Fonctions utiles
-- =============================================================================

-- Fonction pour mettre à jour last_seen
CREATE OR REPLACE FUNCTION update_last_seen()
RETURNS TRIGGER AS $$
BEGIN
  NEW.last_seen = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger pour auto-update last_seen
CREATE TRIGGER trigger_update_last_seen
  BEFORE UPDATE ON installations
  FOR EACH ROW
  EXECUTE FUNCTION update_last_seen();

-- =============================================================================
-- Vue admin pour dashboard
-- =============================================================================

CREATE OR REPLACE VIEW admin_dashboard AS
SELECT
  i.id,
  i.pi_name,
  i.local_ip,
  i.status,
  i.created_at,
  i.last_seen,
  i.installer_version,
  i.alldebrid_configured,
  i.ygg_configured,
  i.cloudflare_configured,
  i.error_message,
  COUNT(l.id) as log_count,
  MAX(l.created_at) as last_log
FROM installations i
LEFT JOIN installation_logs l ON l.installation_id = i.id
GROUP BY i.id;

-- =============================================================================
-- Données de test (optionnel, à supprimer en prod)
-- =============================================================================

-- INSERT INTO installations (pi_name, local_ip, ssh_public_key, ssh_private_key_encrypted, status, installer_version)
-- VALUES ('test-pi', '192.168.1.100', 'ssh-ed25519 AAAA...', 'encrypted...', 'ready', '1.0.0');
