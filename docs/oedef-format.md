# Format `.oedef` v1

Un fichier `.oedef` est un document JSON UTF-8 contenant des définitions ouvertes, jamais un firmware ou un DAMOS propriétaire.

```json
{"format_version":"1.0","ecu":{"manufacturer":"Example","family":"Demo"},"symbols":[{"name":"torque_limit","address":4096,"data_type":"u16"}]}
```

`format_version` est obligatoire. T001 accepte exactement `1.0`; toute autre version est refusée afin d’éviter une interprétation silencieuse. Les versions mineures compatibles devront conserver les champs et sémantiques existants. Une évolution incompatible emploiera une nouvelle version majeure et un convertisseur explicite. Les noms de symboles sont uniques et non vides; `address` est un entier non signé et `data_type` est non vide.
