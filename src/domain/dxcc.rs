//! DXCC prefix table loaded from the on-disk `config/dxcc.txt` file.
//!
//! Each line in the data file maps a DXCC entity name to a comma-separated
//! list of callsign prefixes. Lookup uses the longest matching prefix, so
//! `BA1ABC` resolves to `China` via `BA` while `HB0ABC` resolves to
//! `Liechtenstein` via the more specific `HB0` instead of `HB`.
//!
//! When the file is missing or entirely blank, the built-in
//! [`DEFAULT_DXCC_TABLE`] is written out so the program can start without any
//! manual setup. Delete the file to regenerate it on the next launch.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Embedded prefix table written to disk on first run.
///
/// Format: `ENTITY_NAME: PREFIX1,PREFIX2,...`
/// Lines starting with `#` are comments. Prefixes are matched
/// case-insensitively against the beginning of the callsign.
pub const DEFAULT_DXCC_TABLE: &str = r#"# DXCC prefix -> entity name
#
# Format: ENTITY: PREFIX1,PREFIX2,...
# - Lines starting with # are comments.
# - Prefixes are matched case-insensitively against the beginning of the
#   callsign. The longest matching prefix wins, so keep specific entries
#   (e.g. HB0 -> Liechtenstein) separate from parent prefixes (HB -> Switzerland).
# - This file is auto-created from the embedded default when missing or empty.
# - Restart the program after editing.

# === Asia ===
China: BA,BB,BC,BD,BE,BF,BG,BH,BI,BJ,BK,BL,BM,BN,BO,BP,BQ,BR,BS,BT,BU,BW,BY,BZ
Taiwan: BV,BX
Hong Kong: VR
Macao: XX9
Japan: JA,JB,JC,JD,JE,JF,JG,JH,JI,JJ,JK,JL,JM,JN,JO,JP,JQ,JR,JS,7J,7K,7L,7M,7N,8J,8K,8L,8M,8N
South Korea: HL,DS,DT
North Korea: P5
Mongolia: JT,JU,JV
India: VU,VT,VW
Pakistan: AP
Bangladesh: S2
Sri Lanka: 4S
Nepal: 9N
Bhutan: A5
Thailand: HS,E2
Vietnam: 3W,8P
Cambodia: XU
Laos: XW
Myanmar: XY,XZ
Malaysia: 9M,9W
Singapore: 9V
Indonesia: YB,YC,YD,YE,YF,YG,YH
Philippines: DU,DV,DW,DX,DY,DZ,4D,4E,4F,4G,4H,4I
Brunei: V8

# === Middle East ===
Iran: EP,EQ
Iraq: YI
Israel: 4X,4Z
Jordan: JY
Kuwait: 9K
Lebanon: OD
Oman: A4
Qatar: A7
Saudi Arabia: HZ
Syria: YK
Turkey: TA,TB,TC,YM
United Arab Emirates: A6
Yemen: 7O

# === Europe ===
England: G,M,2A,2B,2C,2D,2E,2I,2J,2U,2W,2X,2Y
Scotland: GM,MM
Wales: GW,MW
Northern Ireland: GI,MI
Ireland: EI,EJ
France: F
Germany: DA,DB,DC,DD,DE,DF,DG,DH,DI,DJ,DK,DL,DM,DN,DO,DP,DQ,DR
Italy: I
Spain: EA,EB,EC,ED,EE,EF,EG,EH
Portugal: CT,CU
Belgium: ON,OO,OP,OQ,OR,OS,OT
Netherlands: PA,PB,PC,PD,PE,PF,PG,PH,PI
Luxembourg: LX
Switzerland: HB,HE
Liechtenstein: HB0
Austria: OE
Denmark: OU,OV,OW,OX,OZ,5P,5Q
Norway: LA,LB,LC,LD,LE,LF,LG,LH,LI,LJ,LK,LL,LM,LN
Sweden: SA,SB,SC,SD,SE,SF,SG,SH,SI,SJ,SK,SL,SM,SN,SO,7S,8S
Finland: OF,OG,OH,OI
Iceland: TF
Poland: HF,HG,HH,HI,HJ,HK,HM,HN,HO,HP,HQ,HR,HS,HT,HU,HV,HW,HX,HY,3Z,SN,SO,SP,SQ,SR,SS,ST,SU,SV,SW,SX,SY,SZ
Czech Republic: OK,OL
Slovakia: OM
Hungary: HA
Romania: YO,YP,YQ,YR
Bulgaria: LZ
Greece: J4,SV,SX,SY,SZ
Croatia: 9A
Slovenia: S5
Serbia: YT,YU
Bosnia and Herzegovina: E7
Montenegro: 4O
North Macedonia: Z3
Albania: ZA
Malta: 9H
Cyprus: 5B,C4,H2,P3
Russia: R,RA,RB,RC,RD,RE,RF,RG,RH,RI,RJ,RK,RL,RM,RN,RO,RP,RQ,RR,RS,RT,RU,RV,RW,RX,RY,RZ,UA,UB,UC,UD,UE,UF,UG,UH,UI,UK,UL,UM,UO
Ukraine: EM,EN,EO,UR,US,UT,UU,UV,UW,UX,UY,UZ
Belarus: EU,EV,EW
Moldova: ER
Estonia: ES
Latvia: YL
Lithuania: LY
Georgia: 4L
Armenia: EK
Azerbaijan: 4J,4K
Kazakhstan: UN,UP,UQ
Uzbekistan: UJ
Turkmenistan: EZ
Tajikistan: EY
Kyrgyzstan: EX
Afghanistan: T6,YA
Monaco: 3A
Vatican City: HV
San Marino: T7
Andorra: C3
Gibraltar: ZB

# === North America ===
USA: K,W,N,AA,AB,AC,AD,AE,AF,AG,AH,AI,AJ,AK,AL,KA,KB,KC,KD,KE,KF,KG,KI,KJ,KK,KM,KN,KO,KQ,KR,KS,KT,KU,KV,KW,KX,KY,KZ,WA,WB,WC,WD,WE,WF,WG,WH,WI,WJ,WK,WL,WM,WN,WO,WQ,WR,WS,WT,WU,WV,WW,WX,WY,WZ
Alaska: KL
Hawaii: KH6
Guam: KH2
Northern Mariana Islands: KH0
American Samoa: KH8
Wake Island: KH9
Midway Island: KH4
Puerto Rico: KP4,NP4,WP4
Canada: VE,VA,VO,VY,VX
Mexico: XA,XB,XC,XD,XE,XF,XG,XH,XI,XJ,4A,4B,4C,6D,6E,6F,6J
Guatemala: TG
Belize: V3
Honduras: HR
El Salvador: YS
Nicaragua: YN
Costa Rica: TI,TE
Panama: HP,HO,3E,3F
Cuba: CL,CM,CO,T4
Jamaica: 6Y
Haiti: HH
Dominican Republic: HI,HJ
Trinidad and Tobago: 9Y,9Z
Bahamas: C6
Barbados: 8P
Bermuda: VP9

# === South America ===
Brazil: PP,PQ,PR,PS,PT,PU,PV,PW,PX,PY,ZV,ZW,ZX,ZY,ZZ
Argentina: LU,LO,LP,LQ,LR,LS,LT
Chile: CA,CB,CC,CD,CE,XQ,XR
Uruguay: CV,CX
Paraguay: ZP
Bolivia: CP
Peru: OA,OB,OC,OD,4T
Ecuador: HC,HD
Colombia: HJ,HK,5J,5K
Venezuela: YV,YW,YY,YZ,4M
Guyana: 8R
Suriname: PZ
French Guiana: FY

# === Oceania ===
Australia: VK,AX
New Zealand: ZL,ZM
Papua New Guinea: P2
Fiji: 3D2
Solomon Islands: H4
Vanuatu: YJ
Samoa: 5W
Tonga: A3
Tuvalu: T2
Kiribati: T3
Nauru: C2
Palau: T8
Micronesia: V6
Marshall Islands: V7

# === Africa ===
South Africa: ZR,ZS,ZT,ZU
Egypt: SU
Algeria: 7X
Morocco: CN
Tunisia: 3V
Libya: 5A
Sudan: ST
Ethiopia: ET
Kenya: 5Y,5Z
Tanzania: 5H,5I
Uganda: 5X
Nigeria: 5N,5O
Ghana: 9G
Senegal: 6V,6W
Ivory Coast: TU
Cameroon: TJ
Angola: D2,D3
Zambia: 9I,9J
Zimbabwe: Z2
Mozambique: C8,C9
Botswana: A2
Namibia: V5
Madagascar: 5R,5S
Mauritius: 3B8
Reunion: FR
Mayotte: FH
"#;

/// Process-wide table installed by [`init_from_path`] or [`init_default`].
static GLOBAL: OnceLock<DxccTable> = OnceLock::new();

/// Lookup table for DXCC entities keyed by callsign prefix.
pub struct DxccTable {
    // Sorted longest-prefix-first so the first hit is also the most specific.
    entries: Vec<(String, String)>,
}

impl DxccTable {
    /// Loads the table from `path`, creating it with [`DEFAULT_DXCC_TABLE`]
    /// when the file is missing or entirely blank.
    pub fn load_or_create(path: PathBuf) -> std::io::Result<Self> {
        let needs_default = match fs::read_to_string(&path) {
            Ok(text) => text.trim().is_empty(),
            Err(_) => true,
        };
        if needs_default {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(&path, DEFAULT_DXCC_TABLE)?;
        }
        let text = fs::read_to_string(&path)?;
        Ok(Self::parse(&text))
    }

    /// Parses the raw text into a table, dropping comments and blank lines.
    fn parse(text: &str) -> Self {
        let mut seen: HashMap<String, String> = HashMap::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((name, prefixes)) = line.split_once(':') else {
                continue;
            };
            let name = name.trim().to_string();
            if name.is_empty() {
                continue;
            }
            for prefix in prefixes.split(',') {
                let prefix = prefix.trim().to_uppercase();
                if prefix.is_empty() {
                    continue;
                }
                seen.entry(prefix).or_insert_with(|| name.clone());
            }
        }
        let mut entries: Vec<(String, String)> = seen.into_iter().collect();
        entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        Self { entries }
    }

    /// Returns the entity name for `call`, or `None` when no prefix matches.
    pub fn entity_for_call(&self, call: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        let upper = call.trim().to_uppercase();
        if upper.is_empty() {
            return None;
        }
        for (prefix, name) in &self.entries {
            if upper.starts_with(prefix.as_str()) {
                return Some(name.as_str());
            }
        }
        None
    }

    /// Returns `true` when the table contains no usable entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Default on-disk location for the DXCC data: `<manifest>/config/dxcc.txt`.
pub fn default_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("config")
        .join("dxcc.txt")
}

/// Loads the table from `path` (creating it if needed) and installs it as the
/// process-wide singleton used by [`entity_for_call`].
pub fn init_from_path(path: PathBuf) -> std::io::Result<()> {
    let table = DxccTable::load_or_create(path)?;
    if table.is_empty() {
        eprintln!("warning: dxcc table is empty; entity lookups will return None");
    }
    let _ = GLOBAL.set(table);
    Ok(())
}

/// Convenience wrapper that loads the table from [`default_path`].
pub fn init_default() -> std::io::Result<()> {
    init_from_path(default_path())
}

/// Returns the DXCC entity for `call`, or `None` if the table is not loaded
/// or no known prefix matches.
pub fn entity_for_call(call: &str) -> Option<String> {
    GLOBAL
        .get()
        .and_then(|t| t.entity_for_call(call).map(|s| s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::DxccTable;

    fn table() -> DxccTable {
        DxccTable::parse(
            "\
China: BH,BY
Taiwan: BV
Japan: JA
Switzerland: HB
Liechtenstein: HB0
Hawaii: KH6
USA: K,W
",
        )
    }

    #[test]
    fn longest_prefix_wins() {
        let t = table();
        assert_eq!(t.entity_for_call("HB9ABC"), Some("Switzerland"));
        assert_eq!(t.entity_for_call("HB0ABC"), Some("Liechtenstein"));
        assert_eq!(t.entity_for_call("KH6ABC"), Some("Hawaii"));
    }

    #[test]
    fn bh8_and_bh7_share_china() {
        let t = table();
        assert_eq!(t.entity_for_call("BH8GUQ"), Some("China"));
        assert_eq!(t.entity_for_call("BH7XYZ"), Some("China"));
    }

    #[test]
    fn case_insensitive_and_unknown() {
        let t = table();
        assert_eq!(t.entity_for_call("ja1abc"), Some("Japan"));
        assert_eq!(t.entity_for_call("ZZ9ZZZ"), None);
    }
}
