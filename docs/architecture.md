# Architecture OpenECU — T001

OpenECU est un workspace Cargo à six crates métier. Les modèles sont purs et n’effectuent ni accès ECU réel ni import de firmware/DAMOS propriétaire.

| Crate | Responsabilité |
|---|---|
| `openecu-core` | identités ECU, matériel, logiciel et provenance |
| `openecu-definitions` | `.oedef` JSON v1 et validation |
| `openecu-calibration` | axes et cartographies de calibration |
| `openecu-firmware` | image firmware en mémoire et lectures bornées |
| `openecu-solutions` | modifications contrôlées et justifiées |
| `openecu-communication` | contrats transport/protocole sans I/O |

`definitions` peut dépendre de `core`; `solutions` peut dépendre de `core`. `calibration` ne dépend pas de `communication`, garantissant que l’édition/calcul reste indépendant du transport. React/Tauri et une IA locale seront des adaptateurs futurs situés au-dessus de ces APIs, sans inverser ces dépendances.
