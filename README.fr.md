# Splinely

[English](README.md) · [Deutsch](README.de.md) · [Español](README.es.md) · **Français** · [日本語](README.ja.md) · [简体中文](README.zh.md)

![Logo de Splinely](assets/splinely-logo.svg)

Splinely est un éditeur de bureau en Rust avec des **fichiers de projet `.sply`** et un runtime JavaScript intégré pour des SVG interactifs. Dessinez formes et tracés, animez-les avec des images clés, des os et des machines à états, puis exportez un seul SVG autonome. **État actuel : un prototype fonctionnel, pas encore tout le périmètre de [PLAN.md](PLAN.md).**

L'éditeur existe en **anglais, allemand, espagnol, français, japonais et chinois simplifié**. Il suit la langue du système et se change dans **Réglages › Général › Langue**. Le japonais et le chinois nécessitent une police CJK installée (p. ex. Noto Sans CJK). Les documents détaillés de `docs/` sont rédigés en allemand.

## Installer avec un LLM

Il suffit de dire à un agent IA disposant d'un terminal (p. ex. Claude Code) :

> Installe-moi Splinely : https://raw.githubusercontent.com/wasser-quest/splinely/main/llms.txt

[`llms.txt`](llms.txt) explique à l'agent comment télécharger le paquet précompilé adapté depuis les releases, vérifier la somme de contrôle, le décompresser dans `~/.local/opt`, puis travailler avec `splinely-cli`.

## Démarrage

**Sans compilation :** chaque [release](https://github.com/wasser-quest/splinely/releases/latest) contient des paquets Linux précompilés (`splinely-vX.Y.Z-x86_64-linux.tar.gz` avec éditeur, CLI, lecteur, runtime et `libsplinely_runtime.so`). Installation : [docs/INSTALL-BINARIES.md](docs/INSTALL-BINARIES.md).

Depuis les sources :

Prérequis : Rust/Cargo ; un environnement graphique/fenêtré fonctionnel pour la fenêtre de l'éditeur. La première compilation télécharge les dépendances. Node.js et Python 3 ne servent qu'aux scripts de vérification (`scripts/check.sh`).

Sous Linux, compiler l'éditeur et le lecteur nécessite les paquets de développement GTK/WebKit, ALSA et D-Bus :

```sh
# Fedora
sudo dnf install gtk3-devel webkit2gtk4.1-devel libsoup3-devel alsa-lib-devel dbus-devel libxkbcommon-devel
# Debian/Ubuntu
sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libasound2-dev libdbus-1-dev libxkbcommon-dev pkg-config
```

```sh
git clone https://github.com/wasser-quest/splinely.git
cd splinely
cargo run -p splinely_editor --bin splinely
# Export sans fenêtre :
cargo run -p splinely_editor --bin splinely -- --demo-export /tmp/splinely-demo.svg
```

L'éditeur démarre avec sa propre démo. **Déposez un fichier `.sply`**, utilisez **Fichier → Ouvrir …** (Ctrl+O, boîte de dialogue système) ou **Récents**, ou lancez directement : `cargo run -p splinely_editor --bin splinely -- examples/demo.sply`. Les modifications non enregistrées et les brouillons de dessin/source sont confirmés avant de changer de projet et à la fermeture ; le titre de la fenêtre affiche le fichier et `•` en cas de modifications. **Enregistrer** conserve le projet modifiable ; le SVG n'est qu'un export facultatif.

**Dessiner / Animer / Avancé** séparent dessin, animation et code. Sélection, Plume, Points du tracé et Main levée sont des outils exclusifs ; les options supplémentaires restent repliées. **Aperçu**, **Lecture** et **Ouvrir Splinely Viewer** sont toujours accessibles. [Utilisation, gestes et limites](docs/usability.md).

Le JavaScript personnalisé ne s'exécute qu'après une autorisation explicite par session dans **Avancé → JavaScript**, jamais dans le canevas natif. **Animer → Audio** permet un aperçu limité via `ffplay` des repères audio globaux ; la préparation se fait en arrière-plan.

## Un seul fichier de projet : `.sply`

Géométrie, calques, os, images clés, états, poids de mélange, variables, audio intégré et code personnalisé restent **réunis dans un seul fichier**. Pas besoin de JSON/SVG supplémentaire ni de dossier de ressources. En interne, `.sply` est du JSON lisible et strictement validé ; les anciens projets `.json`/`.splinely.json` restent compatibles. L'application/le runtime et les plugins installés ne font pas partie du fichier. [Format et compatibilité](docs/sply-format.md).

Dans **Animer**, **Animer facilement** (image clé/préréglage) et **Machine à états guidée** (clic/survol, visibilité, lancement de clip) guident les premiers pas. Le mélange pondéré et les fondus temporisés restent disponibles à côté des outils experts. [Pas à pas et limites du mélange](docs/animation-workflow.md).

```sh
splinely-cli new --demo --output scene.sply
splinely-cli convert legacy.splinely.json --output scene.sply
splinely-viewer scene.sply
# Uniquement pour du code personnel vérifié :
splinely-viewer examples/desert-stickfight.sply --trusted-js
```

## Splinely Viewer

`cargo build -p splinely_viewer` compile la vraie WebView SVG native. Dans l'éditeur, **▶ Splinely Viewer** ouvre un instantané du projet courant avec runtime, états et audio intégré ; le canevas resvg silencieux reste séparé. JS personnalisé uniquement après autorisation dans Avancé. Directement, sans exporter de SVG : `cargo run -p splinely_viewer --bin splinely-viewer -- examples/brown-owl.sply --trusted-js` (vérifier d'abord le code fourni). Les SVG existants sont aussi pris en charge. Les SVG actifs de tiers ne sont pas fiables. Linux requiert GTK3/WebKitGTK ; [détails et tests](crates/splinely_viewer/README.md).

Les exports SVG contiennent **Created with Splinely**, la version de l'application/du schéma et des métadonnées lisibles par machine (`urn:splinely:metadata`). Aucun filigrane visible ; la mention est modifiable et ne prouve pas l'authenticité.

## Utiliser le runtime

[Recettes du runtime](docs/runtime-cookbook.md) : lecture/pause/redémarrage, horloge de clip et globale, intégration HTML, audio et dépannage. `splinely.snapshot()` renvoie l'état du runtime sous forme de données JSON copiées ; `splinely.reportError(error)` utilise le canal d'erreurs commun avec un historique limité.

## Runtime indépendant du navigateur

`splinely_runtime` fournit le cœur de documents de Splinely sous forme de runtime Rust natif, d'ABI C (`cdylib`) et de processus JSON-lines `splinely-runtime`. D'autres langages peuvent envoyer des événements, piloter le temps/les poids des clips et lire les nœuds/os évalués. L'hôte gère le dessin, la détection des clics et l'audio : pas de navigateur, mais pas non plus d'interpréteur JavaScript SVG général. [API et exemples Python](docs/native-runtime.md).

```sh
cargo build -p splinely_runtime
python3 examples/native-runtime/smoke.py --library target/debug/libsplinely_runtime.so --binary target/debug/splinely-runtime
```

## Travailler avec la CLI

La CLI et l'éditeur partagent le même cœur. Les résultats de la CLI sont en JSON ; les erreurs sont du JSON sur stderr (avec un `hint` pour l'étape suivante) et renvoient un code d'erreur. Aucun fournisseur d'IA, clé d'API ou serveur n'est nécessaire.

**Pour commencer :** `splinely-cli guide` présente le flux de travail avec des recettes testées. Sans `--output`, les commandes d'édition enregistrent dans le fichier d'entrée ; `check` détecte les erreurs courantes (formes masquées, clips jamais lus, texte illisible …) avec une correction exécutable, `batch` exécute plusieurs étapes en tout ou rien, `clip` gère les clips et `on` rend les formes interactives.

```sh
F=/tmp/scene.sply
cargo run -p splinely-cli -- new --output $F --width 800 --height 450 --background "#0b1d3a"
cargo run -p splinely-cli -- node add $F --kind circle --id sun --x 400 --y 420 --radius 60 --fill orange --pivot center
cargo run -p splinely-cli -- keyframe $F --clip sunrise --target sun --property y --time 3 --by -260 --easing ease-out --autoplay --loop
cargo run -p splinely-cli -- on $F --event click --target sun --set sun.fill=gold
cargo run -p splinely-cli -- check $F
cargo run -p splinely-cli -- render $F --time 1.5 --output /tmp/scene.png
```

Avec JSON et merge patches :

```sh
cargo run -p splinely-cli -- new --demo --output /tmp/draft.splinely.json
cargo run -p splinely-cli -- validate /tmp/draft.splinely.json
cargo run -p splinely-cli -- inspect /tmp/draft.splinely.json
cargo run -p splinely-cli -- evaluate /tmp/draft.splinely.json --time 0.5
printf '%s\n' '{"name":"Mon animation"}' > /tmp/splinely-patch.json
cargo run -p splinely-cli -- apply /tmp/draft.splinely.json --patch /tmp/splinely-patch.json --output /tmp/final.splinely.json
cargo run -p splinely-cli -- export /tmp/final.splinely.json --output /tmp/final.svg
```

`schema` fournit un guide du format avec des exemples complets, pas un JSON Schema formel. `apply` utilise des merge patches RFC 7396 : les tableaux sont remplacés entièrement. `evaluate` n'exécute ni actions d'état, ni JS, ni audio ; les clips inactifs ne sont pas lus. Détails : [CLI.md](CLI.md), [CORE_API.md](CORE_API.md). Pour d'autres LLM : [guide de rédaction](LLM.md) et [prompt à copier](prompts/splinely-author.md) ; `schema` et `capabilities` listent les fonctionnalités réellement prises en charge.

## Exemple complet hors ligne

- [examples/demo.splinely.json](examples/demo.splinely.json) : projet modifiable avec un cercle, un mouvement de trois secondes par images clés, un changement d'état au clic et un repère WAV intégré à **1,5 seconde de temps de transport global**.
- [examples/demo.svg](examples/demo.svg) : l'export réel de la CLI avec un court gestionnaire `customJs` explicitement autorisé. Un clic sur le cercle déverrouille l'audio et relance le mouvement et le transport ; au premier clic, le cercle devient orange.
- Le son est **un ton synthétique de 0,4 seconde, proche d'une syllabe, sans paroles**. WAV PCM, mono, 8 kHz, 16 bits ; aucun fichier audio externe.

```sh
# Reproductible : CLI new --demo → apply → validate → export --trusted-js
bash scripts/generate-demo.sh
# Export par défaut : le code est seulement conservé ; le bouton de démarrage déverrouille l'audio sans code utilisateur.
cargo run -p splinely-cli -- export examples/demo.splinely.json --output /tmp/demo-safe.svg
# Uniquement après vérification de votre code :
cargo run -p splinely-cli -- export examples/demo.splinely.json --trusted-js --output /tmp/demo-trusted.svg
```

Ouvrir `demo.svg` directement dans le navigateur en `file://`, cliquer sur **Start audio / animation** puis sur le cercle. Ne pas l'intégrer en `<img>` : les scripts SVG n'y sont pas exécutés. Sans scripts, le graphique initial statique reste visible. Les navigateurs/CSP peuvent bloquer les scripts ou la compilation dynamique ; l'audio exige une vraie interaction et un décodeur compatible.

Le runtime fourni et le fichier d'exemple n'ont besoin d'aucune bibliothèque externe ni ressource réseau. **Ce n'est pas un bac à sable** : le code utilisateur autorisé peut effectuer des requêtes réseau et d'autres actions du navigateur. Par défaut, `customJs` est conservé mais pas exécuté ; `rawSvg` reste de simples métadonnées. Ne pas exporter/ouvrir de projets inconnus avec `--trusted-js`.

## Vecteurs, calques et os

- **Calques :** sélection, ordre, visibilité et verrouillage. Couleurs et transparence dans l'inspecteur.
- **Tracés :** un double-clic sur un bord insère une ancre sans modifier la forme ; Maj lie les tangentes de Bézier symétriquement, Maj+Alt garde la longueur voisine. [Détails](docs/path-editing.md). Modifier la syntaxe des tracés SVG ainsi que les ancres/points de contrôle de Bézier graphiques ; **Plume** dessine par clic et glisser (Entrée termine, Échap annule). **Snap 10** aimante localement les positions au relâchement. Convertir rectangles/cercles en tracés et revenir en arrière tant qu'ils sont inchangés. L'import SVG est volontairement limité et refuse l'apparence non prise en charge au lieu de la perdre silencieusement.
- **Os :** sélectionner/déplacer les articulations, modifier parents et liaisons, animer la rotation. Liaison rigide et IK limitée à deux os pour des articulations enfants directes sans décalage ; pas de déformation de maillage. Charger `examples/rig.splinely.json` ; `examples/rig.svg` montre la même animation dans le navigateur.
- **Navigation :** F/Origine ajuste la scène ; +/- zoome, 0 remet à 100 %. Bouton du milieu ou Espace + bouton gauche déplace la vue sans modifier les formes ; Espace sans glisser lance/arrête au relâchement. Les règles peuvent être affichées/masquées et restent en coordonnées du document. La timeline peut être masquée. **Aperçu** sépare les événements de scène de l'édition.
- **Main levée/tablette :** le mode main levée crée des tracés SVG modifiables, un trait par étape d'annulation. Traits sensibles à la pression seulement si des événements de force sont disponibles ; pas de promesse générale matériel/pilotes. [Notes sur les tablettes](docs/tablet.md).
- **Plugins à déposer :** placer un `splinely-plugin.json` dans le dossier des plugins et redémarrer : outils de formes déclaratifs et palettes se chargent automatiquement, sans compilation. [Format, dossiers et exemples](docs/drop-in-plugins.md) ; aucun script/bibliothèque tiers n'est exécuté automatiquement.
- **Plugins :** extensions ECS Rust statiques avec leurs propres ressources et une phase de décoration. [Modèle](plugin-templates/editor/README.md) ; `splinely-cli plugin` génère du code source mais ne l'exécute jamais. Les plugins natifs sont du code de confiance, pas un bac à sable.

```sh
cargo run -p splinely-cli -- import-svg drawing.svg --output drawing.splinely.json
cargo run -p splinely-cli -- rig-demo --output /tmp/rig.splinely.json
cargo run -p splinely-cli -- export /tmp/rig.splinely.json --output /tmp/rig.svg
```

Créer et disposer des formes avec la CLI, puis vérifier en PNG (même logique que l'éditeur) :

```sh
cargo run -p splinely-cli -- node add scene.sply --kind circle --x 200 --y 150 --radius 40 --fill tomato --output scene.sply
cargo run -p splinely-cli -- arrange align scene.sply --nodes node-1,node-2 --edge top --output scene.sply
cargo run -p splinely-cli -- render scene.sply --output scene.png --width 800
```

Autres commandes de formes, disposition, os, calques et conversion : [CLI.md](CLI.md). `keyframe` ajoute des clés de nœud/os individuelles sans patch de tableau complet.

## Implémenté et à venir

| Implémenté | Non implémenté / reporté |
| --- | --- |
| Rectangle, cercle, texte, tracés, polygone/étoile/ligne ; sélection multiple et par rectangle, poignées d'échelle/rotation, repères intelligents et manuels, menu contextuel, palette de commandes (Ctrl+K), réglages (Ctrl+,) avec raccourcis modifiables, thème clair/sombre, langue de l'interface et grille visible, mode enregistrement (auto-key), historique, cotes près du pointeur, timeline visuelle avec images clés déplaçables et menu des transitions, pelure d'oignon réglable, édition du texte sur le canevas, couleurs du document, modèles pour nouveaux projets, export PNG/GIF, pipette, dégradés linéaires/radiaux, groupes imbriqués (arborescence, export `<g>`), texte en tracé, aligner/répartir/retourner, opérations booléennes préservant les courbes, morphing de tracés, dégradés animés, ordre des calques, copier/coller, couleurs/transparence, conversion, annuler/rétablir | Le morphing entre formes de topologies différentes est approximé (sous-tracés qui grandissent/rétrécissent), sans correspondance exacte des trous |
| Charger/enregistrer via fichier temporaire + renommage ; enregistrement automatique toutes les 30 s avec dialogue de récupération | Migrations historiques du schéma, quotas de ressources complets |
| Timeline, images clés, easing, horloges de clip indépendantes, états parallèles, clips pondérés et fondus enchaînés temporisés (`blendClip`) | Mélange additif, blend tree/blend space visuel |
| Modèle d'états, gardes/actions, graphe d'états modifiable, édition JSON et aperçu natif des états | Connexions libres par glisser, mise en page automatique du graphe |
| Événements du navigateur, API clips/transport, ressources et repères audio intégrés | Exécution JS native ou latence audio garantie ; l'aperçu audio natif ne couvre que les repères globaux via ffplay |
| Export SVG statique/interactif, import vectoriel SVG limité, champs source JS/SVG brut | **Pas d'importateur complet pour des SVG arbitraires** ; texte/images/dégradés/filtres refusés à l'import ; le SVG brut seul reste des métadonnées |
| Projection ECS Bevy, plugins statiques enregistrables, os et liaisons rigides | Plugins natifs dynamiques, skinning de maillage, chaînes IK générales, traduction automatique de systèmes Bevy en code navigateur, aperçu isolé non fiable |

L'aperçu natif rend le SVG avec resvg : tracés, coins arrondis et transformations sont visibles. Le texte utilise les polices système locales et peut différer selon le navigateur. Les formes sont sélectionnées selon leur contour exact ; les formes invisibles (opacité 0) ne sont pas cliquables. Le schéma 1 a été étendu ; les anciens projets se chargent avec des valeurs par défaut pour les nouveaux champs. Les anciennes versions de Splinely ne comprennent pas ces champs. Le fichier `.sply` (JSON en interne) fait foi ; l'export SVG n'est pas un aller-retour sans perte.

## Vérifications

```sh
bash scripts/check.sh
```

Le script exécute les tests du workspace, les tests du runtime sous Node, la génération/évaluation/export réels de la démo via la CLI et une vérification Python XML/métadonnées/WAV/ressources. Il contrôle aussi les fichiers d'exemple fournis. **Il ne remplace pas un vrai test navigateur/audio.** En complément, `python3 scripts/svg-corpus.py` vérifie des SVG réels installés et `python3 scripts/browser-smoke.py examples/demo.svg /tmp/splinely-svg-corpus/manifest.json` teste Firefox avec comparaison de pixels. Les tests couvrent le JSON strict, les tracés, calques, os, le cycle de vie des plugins, les régressions de sécurité du runtime et chaque langue de l'interface. La sortie audio réelle et sa latence ne sont pas vérifiées. Reproduction et recette manuelle : [docs/browser-tests.md](docs/browser-tests.md). Captures de l'interface native et vérification des artefacts : [docs/verification.md](docs/verification.md).

## Démos animées

[Vue d'ensemble](examples/showcase.html) · [SVG interactifs, événements directs et états invisibles](docs/interactive-svg.md)

- **[Sand / Accord](examples/desert-stickfight.html)** ([`.sply` modifiable](examples/desert-stickfight.sply)) : court métrage original de 20 secondes avec deux personnages à os dans le style désertique de Cactus Runner, cinq phases de combat et une fin amicale ; lecture/pause/rejouer. [Projet, génération et test réel sous Firefox](docs/desert-stickfight.md).
- [Cactus Runner](examples/cactus-runner.html) : personnage à os qui court, cactus qui défilent et saut au clic avec un vrai mélange de poids marche/saut. [Détails/test](docs/cactus-runner.md).
- [Cinder / Flux – Stickfight](examples/stickfight.html) : combat animé original de huit secondes entre deux rigs avec esquives/contre-attaques, ombres/étincelles et lecture/pause/redémarrage. `python3 scripts/generate-stickfight.py`, test : `python3 scripts/stickfight-smoke.py`.
- [Chouette brune](examples/brown-owl.html) : une chouette sur une branche avec des états directs clic/survol ; au survol, les yeux suivent le pointeur et les aigrettes se dressent ; un clic lui fait fermer les yeux et lever les ailes. Test : `python3 scripts/owl-smoke.py`.
- [Aoi dit bonjour](examples/anime-hello.html) : personnage original inspiré de l'anime avec une main articulée. **Un WAV généré localement est intégré au SVG** ; aucune connexion nécessaire. Un clic déverrouille l'audio. Reproduire : `python3 scripts/generate-anime-demo.py` (nécessite `espeak-ng`/`espeak` en local) ; test navigateur : `python3 scripts/anime-smoke.py`.
- [Goutte d'eau sur une sphère bleue](examples/water-drop.html) : scène SVG autonome avec squash/stretch, reflets et ondulation ; lecture/pause.
- [Figures aléatoires](examples/random-figures.html) : reproductibles par graine, enchaînement automatique d'états avec légendes. Cliquer sur **Play**. Les changements d'état suivent la fin des clips.
- Les deux fichiers HTML intègrent le SVG voisin via `object` ; aucune API serveur. Le serveur de test n'est qu'une aide locale.
- Générer : `splinely-cli water-drop --help`, `splinely-cli random-figures --help` ; exemples complets dans [CLI.md](CLI.md). Test navigateur : `python3 scripts/showcase-smoke.py --duration 35`.

## Logo

Logo SVG original : un S dessiné comme une spline de Bézier avec ancres, poignée et traînée de mouvement.

## Licence

MIT, voir [LICENSE](LICENSE).
