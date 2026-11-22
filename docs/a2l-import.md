# Import A2L en lecture seule (T002-A)

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
