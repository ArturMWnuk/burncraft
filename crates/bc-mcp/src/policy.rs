//! Polityka bezpieczeństwa wywołań MCP (ADR 0005, sekcja 15.4).

use bc_commands::SafetyTier;

#[derive(Debug, Clone)]
pub struct Policy {
    /// Najwyższy poziom wykonywany bez potwierdzenia (domyślnie `design`).
    pub auto_allow: SafetyTier,
    /// Połączenie nielokalne (np. HTTP w LAN). Dla takich `fire` zawsze wymaga potwierdzenia.
    pub remote: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            auto_allow: SafetyTier::Design,
            remote: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Wymaga potwierdzenia użytkownika (GUI lub elicitation).
    NeedsConfirmation(SafetyTier),
}

impl Policy {
    pub fn check(&self, tier: SafetyTier) -> Decision {
        if self.remote && tier == SafetyTier::Fire {
            return Decision::NeedsConfirmation(tier);
        }
        if tier <= self.auto_allow {
            Decision::Allow
        } else {
            Decision::NeedsConfirmation(tier)
        }
    }

    /// Komunikat odmowy. Ścieżki potwierdzenia: motion — elicitation albo zgoda przy
    /// uruchomieniu serwera (M2); config/fire w GUI i elicitation — M6.
    pub fn denial_message(&self, id: &str, tier: SafetyTier) -> String {
        let how = match tier {
            SafetyTier::Motion => "wymaga potwierdzenia użytkownika: klient MCP z obsługą elicitation zapyta użytkownika o każdy ruch, albo użytkownik dopuszcza ruch przy uruchomieniu serwera (burncraft-cli mcp --allow-motion)",
            _ => "wymaga jawnego potwierdzenia w oknie BurnCraft (miniatura, czas, moc, materiał, urządzenie); ta wersja serwera nie wykonuje przez MCP poleceń config i fire",
        };
        format!(
            "odmowa: polecenie {id} ma poziom bezpieczeństwa „{}” i {how}",
            tier.as_str()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn req_15_092_tiers_ordered() {
        use SafetyTier::*;
        assert!(Read < Design && Design < Motion && Motion < Config && Config < Fire);
    }

    #[test]
    fn req_15_093_default_read_design_allowed() {
        let p = Policy::default();
        assert_eq!(p.check(SafetyTier::Read), Decision::Allow);
        assert_eq!(p.check(SafetyTier::Design), Decision::Allow);
        assert_eq!(
            p.check(SafetyTier::Motion),
            Decision::NeedsConfirmation(SafetyTier::Motion)
        );
        assert_eq!(
            p.check(SafetyTier::Fire),
            Decision::NeedsConfirmation(SafetyTier::Fire)
        );
    }

    #[test]
    fn fire_never_auto_for_remote() {
        let p = Policy {
            auto_allow: SafetyTier::Fire,
            remote: true,
        };
        assert_eq!(p.check(SafetyTier::Config), Decision::Allow);
        assert_eq!(
            p.check(SafetyTier::Fire),
            Decision::NeedsConfirmation(SafetyTier::Fire)
        );
    }
}
