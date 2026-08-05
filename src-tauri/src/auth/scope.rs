#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Read,
    WriteNotes,
    WriteMemory,
    Admin,
    Backup,
    Export,
    Maintenance,
}

impl Scope {
    fn bit(&self) -> u16 {
        match self {
            Scope::Read => 1 << 0,
            Scope::WriteNotes => 1 << 1,
            Scope::WriteMemory => 1 << 2,
            Scope::Admin => 1 << 3,
            Scope::Backup => 1 << 4,
            Scope::Export => 1 << 5,
            Scope::Maintenance => 1 << 6,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::Read => "read",
            Scope::WriteNotes => "write_notes",
            Scope::WriteMemory => "write_memory",
            Scope::Admin => "admin",
            Scope::Backup => "backup",
            Scope::Export => "export",
            Scope::Maintenance => "maintenance",
        }
    }

    fn parse(s: &str) -> Option<Scope> {
        match s {
            "read" => Some(Scope::Read),
            "write_notes" => Some(Scope::WriteNotes),
            "write_memory" => Some(Scope::WriteMemory),
            "admin" => Some(Scope::Admin),
            "backup" => Some(Scope::Backup),
            "export" => Some(Scope::Export),
            "maintenance" => Some(Scope::Maintenance),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScopeSet(u16);

impl ScopeSet {
    pub fn empty() -> Self {
        ScopeSet(0)
    }

    pub fn insert(&mut self, s: Scope) {
        self.0 |= s.bit();
    }

    pub fn contains(&self, s: Scope) -> bool {
        self.0 & s.bit() != 0
    }

    pub fn from_csv(csv: &str) -> Self {
        let mut set = Self::empty();
        for part in csv.split(',') {
            if let Some(s) = Scope::parse(part.trim()) {
                set.insert(s);
            }
        }
        set
    }
}
