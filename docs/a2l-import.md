# Import A2L en lecture seule (T002-A)

## Extension T002-B : lecture de calibration BIN

`openecu-calibration` adapte une `CHARACTERISTIC` A2L vers une
`CalibrationDefinition` uniquement lorsque les métadonnées nécessaires sont
présentes : adresse ECU, `RECORD_LAYOUT` référencé avec `FNC_VALUES`, ordre des
octets explicite dans la caractéristique ou `MOD_COMMON`, et `COMPU_METHOD`
référencé. Les types A2L `UBYTE`, `SBYTE`, `UWORD`, `SWORD`, `ULONG` et
`SLONG` correspondent respectivement aux lectures U8, S8, U16, S16, U32 et
S32. `MSB_FIRST` est traité big-endian et `MSB_LAST` little-endian.

Seule la conversion `IDENTICAL` est exécutable ; toute autre conversion est
rejetée par une erreur explicite, jamais approximée. Les valeurs lues gardent
séparément la valeur brute, la valeur physique, l’unité et la provenance
(`ecu_address`, offset BIN, longueur). Les matrices ont des dimensions et une
disposition `RowMajor` explicites.

### Mapping mémoire multi-segments

Le mapping adresse ECU → offset BIN est fourni séparément par l’appelant sous
forme d’`EcuAddressMap`, un ensemble immuable de `MemoryRegion`
(`ecu_start`, `length`, `bin_offset`). Construction validée : région non vide,
aucun débordement de l’espace d’adresses ECU (`u64`) ni des offsets BIN,
au moins une région, aucun chevauchement ni côté ECU ni côté BIN (les régions
adjacentes sont permises, l’ordre de saisie est libre). Une lecture doit
tenir **entièrement dans une seule région** : une adresse hors région (trou)
donne `AddressOutsideMap`, une plage qui dépasse la fin de sa région (même vers
une région adjacente) donne `RangeCrossesRegion`. `check_fits_image(len)`
vérifie que toutes les régions tiennent dans une image donnée.

### Lecture A2L : ce qui est accepté

- Types de caractéristique : `VALUE` (scalaire) et `VAL_BLK` uniquement.
- `RECORD_LAYOUT` : `FNC_VALUES` unique, seul élément de données, à la
  position ASAP2 `1` (position 0, ≥ 2 ou illisible → `UnsupportedA2lLayout` ;
  position absente → `MissingA2lMetadata`). Seule la position 1 démontre que
  les données commencent à l’adresse de la caractéristique.
  `ALIGNMENT_BYTE/WORD/LONG/INT64/FLOAT16_IEEE/FLOAT32_IEEE/FLOAT64_IEEE` : une
  valeur puissance de 2, une seule fois par mot-clé. L’alignement du type de
  `FNC_VALUES` doit diviser l’adresse, sinon rejet ; il ne modifie jamais les
  offsets (élément unique à l’offset 0, tableau compact, aucun remplissage
  entre éléments). Mot-clé inconnu, valeur invalide ou doublon → rejet.
  Limite : l’alignement du début d’enregistrement n’est vérifié que sur
  l’adresse ECU déclarée ; aucun élément de remplissage n’est modélisé. Mode d’index `ROW_DIR` (seul justificatif de
  `RowMajor`) et type d’adresse `DIRECT`. `COLUMN_DIR`, `ALTERNATE_*`,
  `PBYTE/PWORD/PLONG`, ou tout autre élément (`AXIS_PTS_*`, `NO_AXIS_PTS_*`…)
  → `UnsupportedA2lLayout`. Mode d’index ou type d’adresse absent →
  `MissingA2lMetadata`.
- Dimensions d’un `VAL_BLK` : `MATRIX_DIM` ou `NUMBER` (absence → erreur). Seuls
  les blocs à une dimension effective sont acceptés ; plusieurs dimensions
  > 1 sont refusées car l’ordre de stockage n’est pas certain.
- Avant toute allocation, `read_matrix` calcule la taille totale en octets avec
  arithmétique contrôlée, la valide contre la région et l’image, et le nombre
  d’éléments est plafonné à `MAX_MATRIX_ELEMENTS` (1 048 576, erreur
  `MatrixTooLarge`).

## Limitations restantes (T002-B)

- Axes : `CURVE`, `MAP`, `CUBOID`, `CUBE_4/5`, `AXIS_PTS` partagés, `AXIS_DESCR`
  → `UnsupportedA2lCharacteristicType`. L’importeur ne conserve ni les
  positions des éléments du `RECORD_LAYOUT` ni les types d’axes, donc aucun axe
  n’est reconstruit (jamais deviné). `Axis`/`CalibrationMap` restent des
  modèles construits à la main.
- Matrices A2L multi-dimensionnelles et `COLUMN_DIR` non pris en charge.
- Seule la conversion `IDENTICAL` ; `LINEAR`, `RAT_FUNC`, `TAB_*` rejetées.
- Types A2L 64 bits et flottants (`FLOAT32/64_IEEE`, `UINT64`) non pris en
  charge.
- Pas de `BIT_MASK`, `ASCII`, pas de checksums, pas de gestion d’un `SYMBOL_LINK`
  ni de pages/segments (`MEMORY_SEGMENT`) A2L : le mapping est fourni à la main.
- Un bloc ne peut pas traverser deux régions, même adjacentes.

Cette extension reste en mémoire et en lecture seule : elle ne change pas le
format `.oedef` v1, ne modifie pas les octets de firmware et ne communique avec
aucun ECU.

## Périmètre

`openecu-definitions` fournit `parse_a2l(&str)` et `parse_a2l_file(path)`. Le
second lit un fichier strictement UTF-8. L'import est en lecture seule : il ne
communique pas avec un ECU, ne lit pas de firmware et ne transforme pas un A2L
en `.oedef`. Le format `.oedef` v1 reste inchangé.

La première étape de T002 reconnaît les blocs `PROJECT`, `MODULE`,
`MOD_COMMON`, `CHARACTERISTIC`, `AXIS_PTS`, `COMPU_METHOD`, `RECORD_LAYOUT` et
`MEASUREMENT`. Elle extrait, lorsqu'ils sont présents, les noms,
descriptions, version ASAM, adresses, types de caractéristique, références de
layout et de conversion, axes, dimensions `MAX_AXIS_PTS`, unités de méthodes
de conversion et types numériques de mesures. Les arguments d'une
`CHARACTERISTIC` suivent l'ordre ASAP2 : `Name`, `LongIdentifier`, `Type`,
`Address`, `Deposit`, `MaxDiff`, `Conversion`, `LowerLimit`, `UpperLimit`.

Les blocs et mots-clés non pris en charge sont conservés comme données brutes
et donnent un diagnostic `Unsupported`; ils ne reçoivent aucune interprétation
inventée. Les blocs bruts sont stockés une fois, de façon canonique, dans
`A2lDocument::raw_blocks`. Les références `RawBlockId` de `unknown_blocks`,
des modèles et de `RawElement::Block` pointent vers ce stockage; les éléments
restent ainsi dans l'ordre source sans recopier récursivement les sous-arbres.

## Architecture

Le lexer parcourt les caractères Unicode de texte UTF-8 déjà validé, reconnaît les commentaires
`/* ... */`, les chaînes entre guillemets (avec échappement simple) et les
jetons A2L, avec ligne et colonne. Il n'utilise pas d'expression régulière pour
l'analyse.

Le parseur construit un arbre de blocs `/begin` et `/end` au moyen d'une pile
explicite; il vérifie la limite de 128 blocs avant toute descente et retourne
ainsi un diagnostic contrôlé sur une entrée profondément imbriquée. Les arités
des déclarations connues permettent de répartir leurs arguments positionnels
sur plusieurs lignes sans absorber les mots-clés internes de leur déclaration;
ces mots-clés sont reconnus dans l'espace de noms de `CHARACTERISTIC`,
`AXIS_PTS`, `MEASUREMENT`, `COMPU_METHOD` ou `RECORD_LAYOUT` concerné. Cela
inclut notamment `ARRAY_SIZE`, `MAX_GRAD`, `READ_WRITE` et
`REF_MEMORY_SEGMENT`. Une déclaration incomplète est alors une erreur
bloquante. Il accepte `ASAP2_VERSION` avant `PROJECT`, en extrait la version et
conserve tous les éléments racine (`RawItem` et références de `RawBlock`) dans
`A2lDocument::root_elements`, dans leur ordre et à leurs positions source. Il
exige exactement un bloc `PROJECT` racine et diagnostique toute racine
supplémentaire.
Une projection transforme ensuite les
blocs connus vers le modèle intermédiaire `A2lDocument`, `Project`, `Module`,
`ModCommon`, `Characteristic`, `AxisPoints`, `CompuMethod`, `RecordLayout` et
`Measurement`. Les huit en-têtes reconnus sont eux aussi conservés une fois
dans le stockage canonique, avec les positions de chacun de leurs arguments.

Enfin, une validation détecte les déclarations incomplètes, nombres non
signés invalides ou hors plage `u32`, identifiants dupliqués et références
absentes vers les conversions, layouts et axes. `NO_COMPU_METHOD` est réservé
aux références de conversion et `NO_RECORD_LAYOUT` aux références de layout.

## Diagnostics

Les erreurs syntaxiques bloquantes (UTF-8 invalide, commentaire ou chaîne non
terminé, bloc non terminé ou mal apparié) sont retournées sous la forme de
`A2lImportError` avec ligne et colonne.

Les problèmes sémantiques non destructifs sont dans
`A2lDocument::diagnostics` :

- `Severity::Error` : déclaration incomplète, valeur numérique invalide,
  doublon ou référence absente;
- `Severity::Warning` : réservé aux avertissements de compatibilité;
- `Severity::Unsupported` : bloc ou mot-clé préservé mais non interprété.
  Chaque élément brut non interprété reçoit exactement un tel diagnostic;
  aucun diagnostic `Unsupported` n'est émis pour un mot-clé interne pris en
  charge, même s'il reste conservé comme donnée brute.

`A2lDocument::has_errors()` indique la présence d'erreurs sémantiques. Aucune
adresse, unité, conversion ou taille de cartographie n'est inférée lorsqu'elle
manque. Les diagnostics sémantiques utilisent l'emplacement de la déclaration,
de la référence ou de la valeur concernée lorsqu'il est disponible. Les
diagnostics produits après un `/begin` ou `/end` incomplet utilisent la position
réelle de fin de fichier. Chaque
sous-bloc inconnu est signalé, y compris dans `MOD_COMMON`, `AXIS_PTS`,
`COMPU_METHOD`, `RECORD_LAYOUT` et `MEASUREMENT`. Plusieurs `MOD_COMMON` d'un
même `MODULE` sont une erreur; le premier est projeté et les suivants restent
préservés bruts.

## Exemple

```rust
use openecu_definitions::{parse_a2l, Severity};

let document = parse_a2l(include_str!("../tests/fixtures/valid.a2l"))?;
let module = &document.project.modules[0];
assert_eq!(module.characteristics[0].address, Some(0x1000));
assert!(!document.diagnostics.iter().any(|d| d.severity == Severity::Error));
# Ok::<(), openecu_definitions::A2lImportError>(())
```

## Limites connues

Ce n'est pas une implémentation intégrale d'ASAM MCD-2 MC. En particulier,
les tables `COMPU_TAB`, conversions rationnelles, `IF_DATA`, `A2ML`, relations
de groupe/fonction, données d'axe complètes, layouts détaillés, encodages hors
UTF-8 et la majorité des mots-clés optionnels restent bruts. Les arguments
multilignes sont pris en charge pour les déclarations connues listées dans le
périmètre; les mots-clés internes ASAP2 connus sont séparés des arguments d'une
déclaration incomplète. Sans information grammaticale supplémentaire, un mot-clé
fournisseur inconnu ne peut pas toujours être distingué universellement d'un
argument positionnel. Les extensions fournisseur conservent leurs arguments sur
leur ligne de déclaration et restent non interprétées.

## Extension T002-C1 : axes et layouts typés (sans décodage CURVE/MAP)

Cette tranche enrichit uniquement le parseur et le modèle ; aucun décodage de
`CURVE`/`MAP` n'est ajouté, et `openecu-calibration` rejette toujours ces types.

### Arguments positionnels audités

Source : connaissance de la spécification ASAP2 1.x, non revérifiée sur le
texte normatif (voir Limites).

| Bloc | Arguments positionnels |
|------|------------------------|
| `AXIS_DESCR` | Attribute, InputQuantity, Conversion, MaxAxisPoints, LowerLimit, UpperLimit (6) |
| `AXIS_PTS` | Name, LongIdentifier, Address, InputQuantity, Deposit (layout), MaxDiff, Conversion, MaxAxisPoints, LowerLimit, UpperLimit (10, inchangé) |
| `CHARACTERISTIC` | Name, LongIdentifier, Type, Address, Deposit, MaxDiff, Conversion, LowerLimit, UpperLimit (9, inchangé) |
| `RECORD_LAYOUT` | Name (1) |
| `MOD_COMMON` | Comment (1) |

`AXIS_DESCR` avait une arité 0 (arguments limités à la ligne de déclaration) ;
elle est maintenant de 6 : les arguments peuvent s'étendre sur plusieurs lignes
sans absorber les mots-clés internes.

### Mots-clés internes ajoutés

- `AXIS_DESCR` : `BYTE_ORDER`, `CURVE_AXIS_REF`, `DEPOSIT`, `EXTENDED_LIMITS`,
  `FIX_AXIS_PAR`, `FIX_AXIS_PAR_DIST`, `FIX_AXIS_PAR_LIST`, `FORMAT`, `MAX_GRAD`,
  `MONOTONY`, `PHYS_UNIT`, `READ_ONLY`, `STEP_SIZE`. `MAX_AXIS_PTS` n'est pas un
  mot-clé ASAP2 d'`AXIS_DESCR` (le maximum est le 4ᵉ argument) ; il reste
  reconnu uniquement parce que les fixtures T002-A/B l'utilisent.
- `CHARACTERISTIC` : `MATRIX_DIM`, `DISCRETE` (`MATRIX_DIM` était auparavant
  signalé `Unsupported` à tort).
- `AXIS_PTS` : `MONOTONY`.
- `MOD_COMMON` : `ALIGNMENT_*`, `DATA_SIZE`, `DEPOSIT`, `S_REC_LAYOUT`.
- `RECORD_LAYOUT` : `ALIGNMENT_*`, `STATIC_RECORD_LAYOUT` et, pour chacun des
  axes `X`, `Y`, `Z`, `4`, `5` : `AXIS_PTS_`, `AXIS_RESCALE_`,
  `FIX_NO_AXIS_PTS_`, `DIST_OP_`, `NO_AXIS_PTS_`, `NO_RESCALE_`, `OFFSET_`,
  `RIP_ADDR_`, `SHIFT_OP_`, `SRC_ADDR_` ; plus `FNC_VALUES`, `IDENTIFICATION`,
  `RESERVED`, `RIP_ADDR_W`.

### Modèle additif

- `Characteristic::axis_descriptions: Vec<AxisDescription>` : attribut
  (`AxisAttribute::{Std, Fix, Com, Res, Curve, Unknown(texte)}`), grandeur
  d'entrée, conversion, `max_axis_points`, limites (`A2lNumber` : valeur
  finie + texte source), `AXIS_PTS_REF` (axe partagé), `CURVE_AXIS_REF`,
  `FIX_AXIS_PAR`, `FIX_AXIS_PAR_DIST`, `FIX_AXIS_PAR_LIST`, unité, format,
  monotonie, `READ_ONLY`, arguments bruts, positions source des références et
  `raw_block`. Toute valeur absente reste `None`.
- `Characteristic::dimensions` et `axis_references` gardent leur sens T002-B :
  `dimensions` ne provient toujours que du mot-clé historique `MAX_AXIS_PTS`
  (le maximum positionnel est dans `AxisDescription::max_axis_points`).
- `AxisPoints` : `input_quantity`, `lower_limit`, `upper_limit`, `phys_unit`.
- `RecordLayout::elements: Vec<RecordLayoutElement>` : mot-clé, position, type
  de données, `index_mode` (`FNC_VALUES`), `index_increment` (`AXIS_PTS_*`,
  `AXIS_RESCALE_*`), type d'adressage, paires de rééchelonnage, nombre de
  points fixe, arguments et positions source, dans l'ordre source. Les
  positions ne sont ni triées ni vérifiées (unicité, continuité).
- Les nouveaux champs sont `#[serde(default)]`. `.oedef` v1 est inchangé.

### Diagnostics

- `AXIS_DESCR` sans aucun argument (forme historique des fixtures) :
  `Warning`, champs positionnels `None`. Entre 1 et 5 arguments : `Error`.
- Attribut inconnu : conservé tel quel (`Unknown`), `Unsupported`.
- `COM_AXIS` sans `AXIS_PTS_REF` : `Warning` seulement (obligatoriété non
  vérifiée) ; aucun axe n'est supposé.
- Conversion d'un `AXIS_DESCR` absente (hors `NO_COMPU_METHOD`), `AXIS_PTS_REF`
  ou `CURVE_AXIS_REF` absents : `Error`, à l'emplacement de la référence.
- Élément de `RECORD_LAYOUT` incomplet ou position illisible : `Error`, mais
  l'élément est conservé avec les champs lisibles ; `RECORD_LAYOUT` sans nom :
  `Error`.
- Mots-clés et blocs inconnus : conservés dans `extra`/`raw_blocks`, avec un
  diagnostic `Unsupported` unique.

### Limites et incertitudes

- Schémas d'arguments issus de la connaissance d'ASAP2 1.x, non comparés au
  texte normatif ; en particulier les types signé/non signé de `FIX_AXIS_PAR*`
  (lus `i64`, `i64`, `u32`) et le caractère obligatoire de `AXIS_PTS_REF`.
- `InputQuantity` n'est pas validée contre les `MEASUREMENT`.
- `AXIS_DESCR` partiel suivi d'un mot-clé fournisseur inconnu sur la ligne
  suivante : pour les arguments numériques (4ᵉ à 6ᵉ : points max., limites
  inférieure/supérieure), un jeton non numérique placé sur une ligne ultérieure
  n'est pas absorbé ; il devient un mot-clé (`Unsupported`) et la déclaration est
  signalée incomplète. Pour les arguments non numériques (attribut, grandeur
  d'entrée, conversion), la grammaire ne permet pas de distinguer un identifiant
  d'un mot-clé fournisseur : le jeton est alors absorbé comme argument (aucune
  donnée perdue, mais mal attribuée) — limite connue, non devinée. Un jeton
  non numérique sur la même ligne reste une erreur « invalid number ».
- `A2lNumber` : la désérialisation revalide l'invariant (texte fini, valeur
  identique au texte) ; un document sérialisé altéré est rejeté.
- Les `AXIS_DESCR` ne sont projetés que dans `CHARACTERISTIC` ; aucun lien n'est
  établi avec les éléments du `RECORD_LAYOUT` (phase suivante).
