# TODO.md — Kehitysaskeleet

Tämä lista pilkkoo trango-projektin pieniin, itsenäisiin askeliin. Jokainen askel
tuottaa jotain **ajettavaa ja testattavaa** — ei koskaan puolivalmista väliaskelta.
Askeleet edetään järjestyksessä ylhäältä alas; älä hyppää eteenpäin ennen kuin
edellinen on "valmis"-kriteerien mukainen.

Katso [CLAUDE.md](CLAUDE.md) kehitystavasta (TDD, skriptit, git-työnkulku,
Rust-konventiot) ja [SPEC.md](SPEC.md) täydestä tuotespeksistä (näkymät,
tilat) sekä [STYLE.md](STYLE.md) (design-tokenit). Loput tämän tiedoston
"README:n" viittaukset kirjoitushetken speksiin tarkoittavat samaa sisältöä,
joka asuu nykyään SPEC.md/STYLE.md:ssä.

## Jokaisen askeleen "valmis"-kriteerit (toistuu joka kohdassa)

1. Testi kirjoitettu ensin (red), toteutus sen jälkeen (green), siivous (refactor)
2. `scripts/check.sh` → `OK`
3. `scripts/test.sh` → `OK`
4. `Cargo.toml`-versio bumpattu (patch) + versio näkyy UI:ssa/CLI:ssä
5. `releasenotes.md` päivitetty (`[Unreleased]` → oikeat rivit `Added`/`Changed`/`Fixed`/`Removed`)
6. Kyseiseen askeleeseen liittyvä `docs/`-sivu päivitetty, jos askel vaikuttaa arkkitehtuuriin/riippuvuuksiin/käyttöön
7. commit + push

Näitä ei toisteta jokaisen askeleen kohdalla erikseen alla — ne pätevät aina.

## Esivalmistelu (ei koodia, kertaalleen)

- [x] Varmista työkalut asennettuna: `rustc`, `cargo`, `mdbook`, `cargo-audit`,
      `cargo-outdated`, ja `libmpv`-kehitysheaderit (esim. `libmpv-dev` /
      `mpv-libs-devel` järjestelmästä riippuen) — `libmpv-rs` tarvitsee nämä
      linkitykseen
- [x] Päätä minimi Rust-versio (MSRV) ja kirjaa se `Cargo.toml`:iin (`rust-version`) — 1.97 (rustup stable)

---

## Vaihe 1 — Rust workspace kuntoon

**Tavoite:** Tyhjä mutta toimiva Cargo-workspace, joka kääntyy ja jota
`scripts/check.sh` / `scripts/test.sh` osaavat jo ajaa.

- Juureen virtuaali-workspace `Cargo.toml` (`[workspace] members = [...]`)
- Kolme jäsenkirjastoa/-binääriä (perustelu alla kohdassa "Crate-jako"):
  - `crates/subtitle` (lib) — tyhjä `lib.rs` + yksi triviaali testi
  - `crates/playback-state` (lib) — tyhjä `lib.rs` + yksi triviaali testi
  - `crates/app` (bin) — paketin nimi `trango` (`[package] name = "trango"` →
    binäärin nimi on automaattisesti `trango`, ajokomento aikanaan `trango`),
    hakemistonimi pysyy `crates/app` kuvaamassa roolia — `main.rs` joka
    tulostaa version (`tracing`-alustus jo tässä)
- Kaikkiin kolmeen: `version = "0.1.0"` synkassa juuren kanssa (workspace-inherited version suositeltavaa: `[workspace.package]` + `version.workspace = true`)
- `cargo fmt`/`cargo clippy`-asetukset (`rustfmt.toml` jos tarvitaan poikkeuksia — älä lisää ilman syytä)

**Voit ajaa/testata:** `cargo run -p trango` tulostaa versionumeron; `scripts/test.sh` → `OK` (kolme triviaalia testiä); `scripts/check.sh` → `OK`.

### Crate-jako — miksi kolme cratea yhden sijaan

- `subtitle`: puhdas parsintalogiikka (SRT ensin), ei riippuvuutta Slintiin tai
  libmpv:hen → nopeat, eristetyt yksikkötestit oikeilla `.srt`-fixtuureilla
- `playback-state`: puhdas tilakone (tila, kursori, tilasiirtymät kuten
  "seuraava lause" / "toista lause" / tilanvaihto Normal↔SentenceBySentence) —
  ei I/O:ta, ei UI:ta → koko lauseiden-välisen navigointilogiikan voi TDD:tä
  ilman Slint-ikkunaa tai videotiedostoa
- `trango` (hakemisto `crates/app`): binääri, joka solmii Slint-UI:n, libmpv:n
  ja yllä olevat kaksi kirjastoa yhteen — tuotenimi on **TrangoPlayer**,
  ajokomento aikanaan `trango`

Tämä jako mahdollistaa sen, että suurin osa liiketoimintalogiikasta (subtitle-
parsinta, cue-navigointi) on testattavissa ilman raskaita Slint/libmpv-
riippuvuuksia, ja pitää yksittäiset tiedostot pieninä (CLAUDE.md: ~200 riviä).

---

## Vaihe 2 — docs/-runko (mdbook)

**Tavoite:** `mdbook`-rakenne pystyssä, jotta seuraavat askeleet voivat
päivittää sitä matkan varrella eikä sitä tarvitse rakentaa jälkikäteen.

- `docs/src/SUMMARY.md` + kansiot `usage/`, `architecture/`, `specs/`, `technology/`
- `architecture/crates.md`: kuvaa Vaiheen 1 crate-jako (nykytila, ei tavoitetilaa)
- `technology/` yksi sivu jo lisätyistä riippuvuuksista (aluksi lähinnä `tracing`)

**Voit ajaa/testata:** `mdbook build docs` onnistuu virheettä; `mdbook serve docs` näyttää sivun selaimessa.

---

## Vaihe 3 — Subtitle-malli: `Cue`-struct + virhetyypit

**Tavoite:** `subtitle`-craten datamalli ilman parsintaa vielä.

- `Cue { index: u32, start: Duration, end: Duration, text: String }`
- `SubtitleError` (`thiserror`): esim. `InvalidFormat`, `IoError`
- Yksikkötestit `Cue`:n rakentamiselle ja perusvalideoinnille (esim. `start < end`)

**Voit ajaa/testata:** `cargo test -p subtitle` läpäisee `Cue`-testit.

---

## Vaihe 4 — SRT-parseri

**Tavoite:** Oikean `.srt`-tiedoston parsinta `Vec<Cue>`:ksi.

- Testifixtuurit: `crates/subtitle/tests/fixtures/*.srt` (validi tiedosto +
  vähintään yksi rikkinäinen/epätyypillinen tapaus: puuttuva rivinvaihto,
  BOM, virheelliset timestampit)
- `parse_srt(&str) -> Result<Vec<Cue>, SubtitleError>`
- Integraatiotesti joka lukee fixtuuritiedoston levyltä ja tarkistaa cue-määrän,
  ajastukset ja tekstit

**Voit ajaa/testata:** `cargo test -p subtitle` parsii oikean `.srt`-tiedoston ja tuottaa oikeat cuet; virheellinen tiedosto palauttaa `Err`.

---

## Vaihe 5 — Käännössubtitlen liittäminen cueen

**Tavoite:** Kaksi `.srt`-tiedostoa (alkuperäinen + käännös) yhdistetään
indeksin/ajastuksen perusteella yhdeksi `Vec<Cue>`:ksi, jossa `translation: Option<String>`.

- `merge_translation(original: Vec<Cue>, translation: Vec<Cue>) -> Vec<Cue>`
- Testit: täysin yhteensopiva pari, käännös puuttuu joltain riviltä,
  käännöstiedostossa eri määrä cueita kuin alkuperäisessä (dokumentoi valittu
  strategia: esim. matchaus ajastuksen päällekkäisyydellä ei indeksillä, koska
  STT-generoidut ja käsin tehdyt tiedostot voivat erota rivimäärältään)

**Voit ajaa/testata:** `cargo test -p subtitle` kattaa yhdistämislogiikan myös epätäydellisillä pareilla.

---

## Vaihe 6 — `playback-state`: tilat ja moodinvaihto

**Tavoite:** Puhdas tilakone `PlaybackMode`-vaihdolle, ilman UI:ta.

- `PlaybackMode { Normal, SentenceBySentence }`
- `PlayerState { mode, cues: Vec<Cue>, current_cue_index: Option<usize>, show_translation: bool }`
- `PlayerState::toggle_mode()`, `set_cues(...)`, `toggle_translation()`

**Voit ajaa/testata:** `cargo test -p playback-state` — moodinvaihto ja käännöstoggle testattu puhtaasti tilamuutoksina (ei playbackia vielä).

---

## Vaihe 7 — `playback-state`: cue-navigointi (seuraava/edellinen/toista)

**Tavoite:** README:n navigointisäännöt (Right/Left/Space) puhtaana logiikkana,
joka palauttaa "mitä pitäisi tehdä" (esim. `SeekCommand { start, end, then_pause }`),
ei itse ajaa mpv:tä.

- `next_cue()`, `previous_cue()`, `repeat_current_cue()` → palauttavat seek-käskyn
- Reunatapaukset: ensimmäinen/viimeinen cue, tyhjä cue-lista, toista-komento
  aina samasta cuesta riippumatta monestiko sitä painetaan (README:n vaatimus)

**Voit ajaa/testata:** `cargo test -p playback-state` kattaa kaikki navigointi-reunatapaukset — koko sentence-by-sentence-ydinlogiikka on nyt todistetusti oikein ilman että yhtään videota on avattu.

---

## Vaihe 8 — Slint-ikkunan runko

**Tavoite:** Ensimmäinen näkyvä UI: tyhjä ikkuna oikealla taustavärillä ja
otsikkopalkilla, joka näyttää `Cargo.toml`-version.

- Lisää `slint`-riippuvuus `crates/app`:iin (paketti `trango`) (kysy käyttäjältä ensin CLAUDE.md:n
  mukaisesti, perustele: tuotteen UI-kehys on jo päätetty README:ssä)
- `.slint`-tiedosto: ikkuna + top bar -placeholder, tausta `#1c1d22`-ish
  design-tokenin mukaan
- `docs/src/technology/slint.md`

**Voit ajaa/testata:** `cargo run -p trango` avaa ikkunan, jossa näkyy versio ja tausta täsmää design-tokeniin.

---

## Vaihe 9 — Top bar: wordmark + segmented control (staattinen)

**Tavoite:** Top bar täyteen visuaaliseen asuun README:n mukaan, mutta ilman
toiminnallisuutta vielä (klikkaus ei tee mitään tai vain vaihtaa paikallista
Slint-tilaa).

- Dot + "TrangoPlayer"-wordmark, segmented control (Normal / Sentence by sentence),
  kaksi ghost-nappia ("Open video…", "Open subtitles…")
- Typografia/värit design-tokenien mukaan (Inter, JetBrains Mono)

**Voit ajaa/testata:** `cargo run -p trango` — top bar näyttää pikselintarkasti mockin (`sketch/design_reference.dc.html#1c`) mukaiselta; segmentin klikkaus vaihtaa visuaalisen aktiivitilan.

---

## Vaihe 10 — Top bar kytketty `playback-state`-tilaan

**Tavoite:** Segmented control ohjaa oikeasti Vaiheen 6 `PlayerState::toggle_mode()`-logiikkaa `trango`-binäärin sisällä (Slint UI ↔ Rust-tila).

- `trango`-crateen tilanhallinta joka omistaa `PlayerState`:n ja päivittää Slint-mallin
- Yksinkertainen integraatiotesti (jos Slint-testaus siihen taipuu — muuten
  vähintään testi joka ajaa saman logiikan kuin nappi kutsuisi)

**Voit ajaa/testata:** `cargo run -p trango` — moodin klikkaus vaihtaa aidosti `PlayerState.mode`-arvoa (varmista esim. `tracing::debug!`-lokilla tai UI-tekstillä).

---

## Vaihe 11 — libmpv: videon avaaminen ja perustoisto

**Tavoite:** Ensimmäinen oikea video pyörimään ikkunassa (ilman subtitle-
integraatiota vielä).

- Lisää `libmpv-rs` (tai vastaava) `crates/app`:iin (paketti `trango`) — kysy käyttäjältä ensin
- Video-frame-alue Slintissä + libmpv render-kontekstin upotus
- CLI-argumentti tai kovakoodattu testivideopolku alkuun (helpottaa manuaalista
  testausta ennen Open Video -dialogia)

**Voit ajaa/testata:** `cargo run -p trango -- polku/video.mp4` — video näkyy ja toistuu ikkunassa.

**Huom:** Tämä on ensimmäinen askel jossa manuaalinen/visuaalinen testaus on
välttämätöntä automaattitestien lisäksi (libmpv-render ei ole helposti
yksikkötestattavissa) — dokumentoi tämä `docs/src/architecture/`-sivulla.

---

## Vaihe 12 — Scrub bar: aika ja edistymä

**Tavoite:** Nykyinen aika / kokonaisaika + edistymäpalkki toimimaan oikean
libmpv-instanssin tilasta.

- Pollaa/kuuntele mpv:n `time-pos`/`duration`-propertyt
- Scrub bar UI design-tokenien mukaan (4px track, accent progress, valkoinen thumb)

**Voit ajaa/testata:** `cargo run -p trango -- video.mp4` — scrub bar liikkuu toiston mukana; ajat näyttävät oikein.

---

## Vaihe 13 — E2E-testien runko (aloitetaan tässä kohtaa CLAUDE.md:n mukaisesti)

**Tavoite:** Ensimmäinen E2E-testi, joka ajaa oikeaa subtitle-parsintaa +
cue-navigointia yhdessä (ei pelkkiä eristettyjä unit-testejä), koska ensimmäiset
todelliset ominaisuudet (parsinta, navigointilogiikka, video-toisto) ovat nyt olemassa.

- `crates/app/tests/e2e_*.rs` (tai workspace-tason `tests/`): lataa oikea
  `.srt`-fixtuuri + oikea (lyhyt, repoon sopiva/generoitu) testivideo, aja
  cue-navigointi läpi ja tarkista lopputila
- Dokumentoi `docs/src/architecture/testing.md`: mitä E2E kattaa, mitä ei
  (esim. ei pikselintarkkaa UI-screenshot-testausta tässä vaiheessa)

**Voit ajaa/testata:** `scripts/test.sh` ajaa myös uuden E2E-testin osana workspace-testisuitea.

---

## Vaihe 14 — Sentence-by-sentence: cuen näyttäminen UI:ssa

**Tavoite:** "Current sentence card" näyttää oikean cuen tekstin videon
ajastuksen mukaan (ei vielä nappien ohjausta).

- `PlayerState.current_cue_index` päivittyy mpv:n `time-pos`-seurannasta
  sentence-by-sentence-moodissa
- Kortti design-tokenien mukaan: "Sentence N / M", original-teksti, divider

**Voit ajaa/testata:** `cargo run -p trango -- video.mp4 subs.srt` — kortti näyttää oikean lauseen playback-ajan mukaan.

---

## Vaihe 15 — Nuolinäppäimet ja space: navigointi kytkettynä

**Tavoite:** Vaiheen 7 puhdas navigointilogiikka kytketään oikeasti näppäimiin
ja mpv:n seek-kutsuihin.

- Right/Left/Space-näppäinkäsittelijät Slintissä → kutsuvat `playback-state`-
  funktioita → tulos ajetaan libmpv:llä (seek + play-to-end + pause)
- Bottom hint bar näkyviin sentence-by-sentence-moodissa

**Voit ajaa/testata:** `cargo run -p trango -- video.mp4 subs.srt` — nuolet ja space toimivat README:n kuvaaman käytöksen mukaisesti (manuaalisesti todennettavissa + navigointilogiikka on jo yksikkötestattu Vaiheessa 7).

---

## Vaihe 16 — Sentence list -kortti + rivin klikkaus

**Tavoite:** Kaikki cuet listana, klikkaus hyppää kyseiseen cueen (sama
käytös kuin nuolinavigointi).

- Scrollattava lista, current-rivin korostus (accent-tint pill)
- Klikkaus → sama `playback-state`-funktio kuin nuolinavigoinnissa (ei
  duplikoitua logiikkaa)

**Voit ajaa/testata:** `cargo run -p trango -- video.mp4 subs.srt` — listan klikkaus vaihtaa nykyisen lauseen ja scrollaa/korostaa oikein.

---

## Vaihe 17 — Käännöstoggle

**Tavoite:** Translation-kytkin näyttää/piilottaa käännösrivin nykyisessä
kortissa; oletuksena piilossa; ei vaikuta toistoon.

- Kytketty `PlayerState.show_translation` (Vaihe 6) + `merge_translation`-
  datamalliin (Vaihe 5)

**Voit ajaa/testata:** `cargo run -p trango -- video.mp4 subs.srt subs.en.srt` — toggle näyttää/piilottaa käännöksen.

---

## Vaihe 18 — Open Video -dialogi (in-app tiedostolista)

**Tavoite:** README:n mukainen modaali: kansiosta löytyvät videotiedostot
listana (ei OS-native file picker — mockin oma UI), valinta + "Open" lataa videon.

- Tiedostolistaus `std::fs`:llä annetusta/oletuskansiosta (kansion vaihto voi
  olla myöhempi lisäys — kirjaa `TODO`-huomautus `docs/`-speksiin, ei koodiin)
- Rivit: tiedostotyyppi-chip, nimi, kesto·koko (kesto/koko vaatii metadata-luvun
  — jos libmpv/ffprobe-tason haku on liian raskas tässä vaiheessa, aloita
  pelkällä tiedostonimellä ja lisää metadata seuraavassa iteraatiossa)
- Auto-match: valinnan yhteydessä etsi samanniminen `.srt` samasta kansiosta

**Voit ajaa/testata:** `cargo run -p trango` (ilman CLI-videoargumenttia) — dialogi avautuu, listaa oikean kansion tiedostot, "Open" lataa valitun videon ikkunaan.

---

## Vaihe 19 — Open Subtitles -dialogi: linkitys ja tyhjä tila

**Tavoite:** README:n mukainen modaali: alkuperäiskielen subtitle löytyy →
linkitetty rivi; ei löydy → dashed empty state + "Generate subtitles" -nappi
(nappi voi vielä olla no-op/stub tässä vaiheessa — generointi on Vaihe 20).
Käännösrivi + drag-and-drop-kohde toiselle `.srt`:lle.

**Voit ajaa/testata:** `cargo run -p trango` — dialogi näyttää oikean tilan (linkitetty/tyhjä) sen mukaan löytyykö tiedosto levyltä; drop kytkee käännöstiedoston.

---

## Vaihe 20 — Subtitle-generointi (stub-rajapinta)

**Tavoite:** `SubtitleGenerator`-trait + `subtitleGenerationStatus`-tila
(`Idle|Generating|Done|Error`) kytkettynä UI:hin, mutta toteutus alkuun
esim. yksinkertaisella/valeaikaisella toteutuksella — **kysy käyttäjältä
ennen minkään STT-kirjaston (esim. Whisper-sidonnan) lisäämistä**, koska tämä
on merkittävä uusi riippuvuus.

**Voit ajaa/testata:** `cargo run -p trango` — "Generate subtitles" -nappi vaihtaa tilan Generating→Done (tai Error) ja UI päivittyy vastaavasti; oikea STT-integraatio voi olla oma erillinen jatkoaskel tämän jälkeen.

---

## Vaihe 21 — Normal-moodin viimeistely

**Tavoite:** Normal-moodi täyteen speksin mukaiseen kuntoon: jatkuva toisto,
scrub bar, sentence-panelin käytös Normal-moodissa (README jättää tarkan
käytöksen tekijän päätettäväksi — dokumentoi valinta `docs/src/specs/`).

- [x] Jatkuva toisto (Space toimii molemmissa moodeissa plain play/pause
  -togglena, ks. `docs/src/developer/specs.md` "Space works in every mode")
- [x] Hint bar näyttää Normal-moodin omat pikanäppäimet (ks.
  `docs/src/developer/specs.md` "Normal mode's own hint bar content")
- [x] Scrub bar raahattavissa/klikattavissa seekaamiseen (ks.
  `docs/src/developer/specs.md` "Scrub bar drag-to-seek")
- [x] Sentence-panel (current-sentence card, sentence list, Ctrl+A word
  analysis) seuraa nyt jatkuvaa toistoa Normal-moodissa live time-pos:in
  perusteella (ks. `docs/src/developer/specs.md` "Normal mode's
  sentence-panel behavior: live time-pos syncing"). Avoimena vain
  laajempi kysymys pitäisikö paneeli ylipäätään näyttää/piiloutua
  Normal-moodissa — nykyinen käytös (aina näkyvissä) säilyy toistaiseksi

**Voit ajaa/testata:** `cargo run -p trango -- video.mp4 subs.srt` — Normal-moodissa toisto jatkuu keskeytyksettä, hint bar näyttää oikean sisällön moodin mukaan, scrub baria voi raahata/klikata seekatakseen.

---

## Vaihe 21.5 — Oikea subtitle-generointi: whisper-cli ulkoisena työkaluna

**Tavoite:** Vaihe 20:n stub-`SubtitleGenerator`:n rinnalle oikea toteutus,
joka kutsuu whisper.cpp:n `whisper-cli`-binääriä ulkoisena prosessina
(`std::process::Command`) — **ei uutta Cargo-riippuvuutta**, koska työkalu
ei linkity trangoon vaan ajetaan erillisenä komentona käyttäjän itse
asentamana (päätetty keskustellen — ks. `docs/src/specs/`, vertailtiin
`whisper-rs`-sidontaan ja muihin STT-vaihtoehtoihin). Whisper.cpp tukee
`-osrt`-lippua, joka kirjoittaa `.srt`-tiedoston suoraan — ei tarvitse
parsia raakatekstiä itse.

Huom: `whisper-cli` **ei ole asennettuna** kehityskoneelle tätä TODO-kohtaa
kirjattaessa (ei PATH:issa, ei pip/apt/snap-asennuksena) — asennus tehdään
vasta kun tämä vaihe otetaan käsittelyyn.

- Uusi `SubtitleGenerator`-toteutus, esim. `WhisperCliGenerator`, joka
  rakentaa ja ajaa `whisper-cli`-komennon (binäärin polku/nimi
  konfiguroitavissa, oletus PATH-haku)
- Ajo taustasäikeessä (`std::thread` + `slint::invoke_from_event_loop`
  tuloksen palauttamiseen), koska oikea transkriptio kestää sekunneista
  minuutteihin — UI ei saa jäätyä (`subtitleGenerationStatus` pysyy
  `Generating`-tilassa ajon ajan)
- Binäärin puuttuminen näkyy selkeänä `Error`-tilana + ymmärrettävänä
  viestinä ("asenna whisper.cpp, ks. ohje"), ei geneerisenä virheenä
- Mallitiedoston (ggml/gguf) polku samoin konfiguroitavissa; lataus/hankinta
  jää käyttäjän vastuulle — dokumentoi asennus-/mallinhankintaohje
  `docs/src/usage/`
- Windows/Linux: `Command::new` toimii molemmilla samalla tavalla, ainoa ero
  on binäärin nimi/asennustapa — dokumentoi molemmat `docs/`

**Voit ajaa/testata:** `cargo run -p trango -- video.mp4` (ilman valmista
subtitlea) — "Generate subtitles" ajaa oikean whisper-cli-transkription
taustalla, tila näkyy `Generating`→`Done`, syntynyt `.srt` latautuu
soittimeen; jos `whisper-cli` puuttuu, tila päätyy `Error`+selkeä viesti.

---

## Vaihe 21.6 — Mallin valinta UI:sta: autodiscovery + persistointi + kieli

**Tavoite:** Vaihe 21.5:n `TRANGO_WHISPER_MODEL_PATH`-ympäristömuuttuja
korvataan UI:sta tehtävällä mallin valinnalla — käyttäjä voi vaihtaa
mallia (esim. eri kielille) käynnistämättä sovellusta uudelleen, ja
sovellus ehdottaa itse todennäköisiä mallikansioita sen sijaan että
käyttäjä joutuisi aina selaamaan kotihakemistosta asti.

- Uusi "Select whisper model" -rivi Open Subtitles -dialogissa
  (`TODO.md` Vaihe 19/20/21.5:n "Generate subtitles" -napin vieressä) —
  avaa in-app-kansioselaimen (sama `FileListDialog`-komponentti kuin Open
  Video -dialogissa / käännöslinkityksessä), josta valitaan `.bin`/`.gguf`-
  tiedosto. "Generate subtitles" -nappi on pois päältä kunnes malli on
  valittu.
- Autodiscovery: selain avautuu oletuksena parhaaseen löydettyyn
  kansioon (muutama yleinen whisper.cpp-asennuspolku + `./models`, ks.
  `crates/app/src/model_picker.rs::candidate_model_folders`) — ei pelkkää
  kotihakemistoa. Käyttäjä voi silti navigoida mihin tahansa kansioon itse.
- Valittu malli **persistoidaan** pieneen TOML-config-tiedostoon
  (`crates/app/src/config.rs`, `$XDG_CONFIG_HOME/trango/config.toml` tai
  `$HOME/.config/trango/config.toml`) — uusi Cargo-riippuvuus `serde`+`toml`
  (kysytty ja hyväksytty käyttäjältä ennen lisäystä, CLAUDE.md:n mukaisesti).
  Muistaa myös viimeksi selatun kansion, jotta selain ei aina palaa
  autodiscoveryn oletukseen.
- Kieli päätellään automaattisesti mallin tiedostonimestä
  (`model_picker::language_flag`): whisper.cpp:n `.en`-päätteiset mallit
  (esim. `ggml-base.en.bin`) ovat englanti-only → `-l en`; muut
  (monikieliset) mallit → `-l auto`, koska whisper-cli:n oma oletuskieli on
  aina `en` riippumatta ladatusta mallista.
- Dokumentoitu `docs/src/usage/`: heikkoresurssisemmat kielet (esim.
  heprea) tarvitsevat ison monikielisen mallin (`medium`/`large-v3`) hyvään
  laatuun — pienet mallit (`base`/`small`) ovat niissä selvästi heikompia.

**Voit ajaa/testata:** `cargo run -p trango -- video.mp4` — Open Subtitles
-dialogissa "Select whisper model" avaa kansioselaimen valmiiksi
todennäköiseen kansioon; valinnan jälkeen "Generate subtitles" aktivoituu
ja ajaa whisper-cli:n oikealla `-l`-lipulla; malli pysyy valittuna myös
seuraavalla käynnistyskerralla.

---

## Vaihe 22 — Design-tarkennus (pikselintarkkuus mockiin)

**Tavoite:** Käy koko UI läpi `sketch/design_reference.dc.html`:ää vasten —
värit, spacing, radiukset, varjot, typografia täsmäävät README:n
Design Tokens -osioon kauttaaltaan.

**Voit ajaa/testata:** Visuaalinen vertailu ajossa olevan sovelluksen ja mockin välillä, kohta kohdalta README:n "Design Tokens" -listaa vasten.

---

## Vaihe 23 — Julkaisukunnostus

**Tavoite:** Ensimmäinen "oikea" versio ulos.

- `scripts/deps-check.sh` ajettu ja läpikäyty (audit + outdated)
- `docs/`-mdbook kokonaisuudessaan ajan tasalla
- `releasenotes.md`: kaikki askeleiden Unreleased-merkinnät koottu ensimmäiseksi
  varsinaiseksi versioksi (esim. `0.1.0` → päätä release-versiointi tässä kohtaa)

**Voit ajaa/testata:** `scripts/check.sh` ja `scripts/test.sh` OK, `mdbook build docs` OK, `cargo audit`/`cargo outdated` ei kriittisiä löydöksiä.

---

## Vaihe 24 — Sana-sanalta-analyysi paikallisella Ollamalla (Ctrl+A)

**Tavoite:** README:n ulkopuolinen, myöhemmin päätetty ominaisuus:
nykyisen lauseen sana-sanalta-analyysi (käännös + ääntämisohje) paikallisen
[Ollama](https://ollama.com)-mallin kautta, Ctrl+A-popupissa. Sama analyysi
voidaan ajaa kaikille lauseille kerralla ja tallentaa tiedostoon uudelleenkäyttöä
varten. Ks. `docs/src/specs/`:n "Word analysis: local Ollama, not a cloud API"
täydelle päätöksentekoketjulle (crate-jako, HTTP-client-valinta, prompt/JSON-
skeema, cache-formaatti).

- Uusi `crates/word-analysis`-kirjastocrate (ei Slint/libmpv-riippuvuutta,
  kuten `subtitle`/`playback-state`): `WordEntry`/`WordAnalysis`-datamalli,
  JSON-sivutiedosto-cache (`cache_path_for`/`load_cache`/`save_cache`,
  avaimena `Cue::index`) ja `OllamaClient`-trait + `ureq`-pohjainen
  `HttpOllamaClient` (`list_models`, `analyze_sentence`) — uudet
  riippuvuudet `ureq` ja `serde_json`, kysytty ja hyväksytty käyttäjältä
  ensin
- `config.rs`: `ollama_model`-kenttä persistoi valitun mallin, samaan
  tapaan kuin `whisper_model_path`
- Open Subtitles -dialogi: "Ollama model" -rivi avaa mallilistan
  (`crates/app/src/ollama_model_picker.rs`, uudelleenkäyttää
  `FileListDialog`-kehystä), listaus taustasäikeessä koska verkkokutsu
- Open Subtitles -dialogi: "Analyze all sentences" -nappi
  (`crates/app/src/word_analysis.rs::spawn_batch_analyze`) looppaa kaikki
  cuet taustasäikeessä, ohittaa jo cachetetut, tallentaa cachen levylle
  jokaisen onnistuneen analyysin jälkeen (ei vasta lopussa)
- Ctrl+A avaa `WordAnalysisPopup`-komponentin (`app-window.slint`) —
  näyttää nykyisen senttenssikortin lauseen sana-sanalta-analyysin;
  cache-hit näyttää tuloksen heti, cache-miss ajaa
  `spawn_analyze_sentence`:n taustasäikeessä ja tallentaa tuloksen samaan
  cache-tiedostoon. Ei moodiriippuvainen, kuten Ctrl+T

**Voit ajaa/testata:** `cargo run -p trango -- video.mp4 subs.srt` — Open
Subtitles -dialogissa Ollama-mallin valinta toimii ja persistoituu;
"Analyze all sentences" ajaa koko subtitlen läpi ja kirjoittaa
`subs.wordanalysis.json`:n; Ctrl+A näyttää nykyisen lauseen sana-sanalta
-analyysin popupissa (cache-hit heti, cache-miss lyhyen latauksen
jälkeen) sekä Normal- että Sentence-by-sentence-moodissa.

---

## Vaihe 24.1 — Sana-analyysin kohdekielen valinta UI:sta

**Tavoite:** Vaihe 24:n kiinteä `"English"`-kohdekieli korvataan
käyttäjän muokattavalla arvolla — sana-analyysin käännökset/ääntämisohjeet
voidaan tuottaa mihin tahansa kieleen, ei vain englanniksi. Ks.
`docs/src/specs/`:n "Target language: free text, not a fixed list"
päätöksenteolle (vapaa tekstikenttä vs. kiinteä kielilista — käyttäjältä
kysytty, valittu vapaa tekstikenttä).

- Open Subtitles -dialogin Word analysis -osioon uusi tekstikenttä
  ("Target language:", Slintin `std-widgets`-`LineEdit` — ensimmäinen
  editoitava tekstikenttä sovelluksessa) Ollama-mallirivin alle
- `config.rs`: `ollama_target_language: Option<String>` persistoi
  syötetyn kielen, samaan tapaan kuin `ollama_model`
- `main.rs::wire_ollama_target_language`: `LineEdit`:n `edited`-callback
  päivittää jaetun `Rc<RefCell<String>>`-tilan ja tallentaa configiin
  jokaisen näppäinpainalluksen jälkeen
- `spawn_batch_analyze`/`spawn_analyze_sentence`-kutsut käyttävät
  kiinteän `word_analysis::DEFAULT_TARGET_LANGUAGE`:n sijaan tätä
  jaettua tilaa — oletusarvo `"English"` näkyy kentässä vain kunnes
  käyttäjä kirjoittaa jotain muuta

**Voit ajaa/testata:** `cargo run -p trango -- video.mp4 subs.srt` — Open
Subtitles -dialogissa "Target language"-kenttä näyttää oletuksena
"English", kirjoitettu arvo säilyy sovelluksen uudelleenkäynnistyksen
yli, ja Ctrl+A/"Analyze all sentences" käyttävät kenttään kirjoitettua
kieltä Ollama-promptissa.

---

## Huom: Vaiheet 25–30 jakavat yhden työhaaran

Poiketen CLAUDE.md:n oletustyönkulusta (oma feature-branch + PR per vaihe):
Vaiheet 25–30 tehdään **samalla työhaaralla** alusta loppuun, koska ne
muodostavat yhden loogisen kokonaisuuden (audiolähteen lisääminen
soittimeen videolähteen rinnalle). Jokaisen vaiheen jälkeen tehdään silti
commit + push samalle haaralle normaalisti (CLAUDE.md:n "valmis"-kriteerit
pätevät muuten sellaisenaan). `gh pr create` ajetaan vasta kun käyttäjä
erikseen pyytää PR:n avaamista — ei automaattisesti minkään yksittäisen
vaiheen jälkeen.

**Suunnanmuutos (2026-07-17):** Alkuperäinen Vaihe 25–31 (versiot
0.1.52–0.1.54) rakensi live-tekstitysgenerointia: jatkuva audiovirta
pilkottiin VADilla puhesegmenteiksi lennossa, ja jokainen segmentti
transkriboitiin omana whisper-cli-kutsunaan heti valmistuttuaan. Tämä
osoittautui tarpeettoman monimutkaiseksi. Suunnitelma yksinkertaistui:
nauhoitus tuottaa yhden eheän äänitiedoston levylle — kuin tavallinen
nauhuri — ja tekstitys generoidaan siitä erillisellä napilla vasta kun
käyttäjä niin haluaa, samalla kertakutsu-periaatteella kuin videosta jo
nyt (Vaihe 20/21.5). Samalla top barin kolmen segmentin
Normal/SentenceBySentence/NoVideo-kontrolli puretaan kahdeksi
riippumattomaksi valinnaksi: Video/Audio-lähde ja Normal/Sentence-by-
sentence-navigointi, joka toimii identtisesti kummassakin lähteessä.
Alla olevat Vaiheet 25–30 korvaavat kokonaan alkuperäiset Vaiheet 25–31;
VAD-segmentointi (`crates/audio-capture/src/vad.rs`, `webrtc-vad`-
riippuvuus) ja live-transkriptio (`crates/app/src/live_transcription.rs`,
suuri osa `crates/app/src/system_audio_capture.rs`:stä) puretaan alla
olevissa vaiheissa sitä mukaa kun tilalle tuleva toteutus valmistuu.

---

## Vaihe 25 — Top bar: Video/Audio-lähdenapit + oma Normal/SbS-toggle

**Tavoite:** Top bar jakaa kaksi toisistaan riippumatonta valintaa: mikä
lähde on käytössä (Video/Audio) ja miten lauseissa navigoidaan (Normal/
Sentence by sentence). Puhdas tilalaajennus/-uudelleenjärjestely, ei vielä
audiokaappausta eikä generointia. Pohjustaa Vaiheet 26–30.

Taustaa (keskusteltu, ei alun perin SPEC.md:ssä): tavoite on tuottaa
tekstitystiedosto ilman että sovellus koskaan lataa tai tallentaa itse
videota/audiota mistään ulkopuolisesta lähteestä (esim. YouTube) — vain
käyttäjän omalta koneelta jo soivan äänen kaappaus tai jo olemassa olevan
paikallisen äänitiedoston avaaminen, ja lopputulos-`.srt` jää talteen.
Sekä YouTuben videon suora toisto/lataus (`yt-dlp`) että YouTuben valmiiden
tekstitysten kaappaaminen harkittiin ja hylättiin tekijänoikeussyistä
(päätös kirjattu `docs/src/developer/specs.md`:hen, ks. myös alla "Ei
tässä listassa").

- `crates/playback-state`: `PlaybackMode` palautuu kahteen arvoon
  (`Normal`, `SentenceBySentence`) — nykyinen `NoVideo`-variantti poistuu
  sieltä kokonaan. Uusi, siitä riippumaton tyyppi kuvaa kumpi lähde/paneeli
  on näkyvissä (esim. `MediaSource { Video, Audio }` — päätä nimi tässä
  vaiheessa). `PlayerState` toimii jo nyt ilman video-riippuvuutta
  (`cues`-kenttä ei koskaan viitannut videoon)
- Top bar: kaksi nappia "Video"/"Audio" valitsemassa lähteen, erillinen
  Normal/Sentence-by-sentence-toggle näkyy ja toimii identtisesti
  molemmissa lähteissä (visuaalinen sijoittelu ei ole mockissa valmiina —
  päätä tässä vaiheessa)
- Nykyinen kolmen segmentin `SegmentButton`-rivi (`crates/app/ui/app-window.slint`,
  `PlaybackModeUi::NoVideo`) puretaan kahdeksi erilliseksi kontrolliksi;
  `crates/app/src/main.rs`:n mode-mäppäys (`to_playback_mode_ui`/`from_...`)
  päivittyy vastaavasti
- Slint: video-widgetin paikalle vaihtoehtoinen paneeli Audio-lähteessä
  (tyhjä placeholder riittää tässä vaiheessa — sisältö tulee Vaiheissa
  27–29)
- Sentence list ja Ctrl+A pysyvät kytkettyinä samaan `PlayerState.cues`:iin
  kuin ennenkin — ei muutoksia niihin tässä vaiheessa (validointi
  Vaihe 30:ssä)

**Voit ajaa/testata:** `cargo test -p playback-state` kattaa Normal/SbS-
tilavaihdon (kahtena arvona) erillään lähdevalinnasta; `cargo run -p
trango` — top barista voi valita Video/Audio-lähteen ja Normal/SbS-
navigoinnin täysin toisistaan riippumatta, Audio-lähteessä video-alue
korvautuu placeholderilla.

---

## Vaihe 26 — Järjestelmä-audion kaappaus yhdeksi eheäksi tiedostoksi

**Tavoite:** Ctrl+Space käynnistää/pysäyttää järjestelmän ulostulevan
äänen (esim. selaimessa soivan videon) nauhoituksen yhdeksi eheäksi
äänitiedostoksi levylle — kuin tavallinen nauhuri, ei live-segmentointia
eikä transkriptiota. Korvaa alkuperäisen Vaihe 26:n stdout-striimauksen.

- `audio_capture::AudioCapture::start`/`stop`: `ffmpeg -f pulse -i
  <monitor-source>` kirjoittaa suoraan kohdetiedostoon (esim. `ffmpeg ...
  output.wav`) sen sijaan että PCM striimataan stdoutiin taustasäikeen
  luettavaksi (0.1.54:n malli puretaan) — monitorilähteen tunnistus
  ennallaan `pactl`:n kautta / `config.toml`:n `audio_monitor_source`
- `crates/audio-capture/src/vad.rs` ja `webrtc-vad`-riippuvuus poistetaan
  kokonaan käyttämättöminä (CLAUDE.md "Ei dead codea") — kirjaa poisto
  `releasenotes.md`:n `Removed`-riville ja poista
  `docs/src/developer/technology/webrtc-vad.md`
- `crates/app/src/live_transcription.rs` poistetaan; `system_audio_capture.rs`
  yksinkertaistuu pelkäksi start/stop-ohjaukseksi ilman per-segmentti-
  whisper-cli-kutsuja
- Edelleen vain Linux/PulseAudio-PipeWire (ei muutosta aiempaan)

**Voit ajaa/testata:** `cargo run -p trango` Audio-lähteessä, Ctrl+Space
käynnistää ja pysäyttää nauhoituksen — manuaalisesti todennettavissa että
yksi WAV-tiedosto syntyy kokonaisuudessaan levylle ja sisältää soineen
äänen (kuten Vaihe 11, ei helposti yksikkötestattavissa
laitteistoriippuvuuden takia).

---

## Vaihe 27 — Rec/stop-ohjaus, tiedostonimi ja tallennuskansio

**Tavoite:** Audio-paneeliin näkyvä rec/stop-kontrolli, tiedostonimen
näyttö ja hallinta, tallennuskansion persistointi.

- Video-widgetin paikalla oleva Audio-paneeli näyttää: rec/stop-tila
  (Ctrl+Space kytkettynä samaan komentoon kuin nappi), kohdetiedoston nimi
- Oletustiedostonimi päiväys+aikaleima (esim. `2026-07-17_18-42-05.wav`),
  lukittu nauhoituksen ajaksi; uudelleennimeäminen sallittu vasta stopin
  jälkeen
- `config.rs`: uusi `audio_recording_folder: Option<PathBuf>` samalla
  periaatteella kuin `video_folder` — muistaa viimeksi käytetyn kansion,
  oletuksena sinne seuraavallakin kerralla

**Voit ajaa/testata:** `cargo run -p trango` — Ctrl+Space tai nappi
käynnistää nauhoituksen, tiedostonimi näkyy paneelissa koko ajan, stopin
jälkeen nimeä voi muokata, ja seuraava nauhoitus ehdottaa oletuksena
samaa kansiota kuin edellinen.

---

## Vaihe 28 — Äänitiedoston avaaminen ja toisto videon tapaan

**Tavoite:** Audio-lähteessä voi nauhoittamisen lisäksi avata olemassa
olevan äänitiedoston (esim. edellisen nauhoituksen) ja toistaa sitä
samoin napein kuin video-lähteessä: seek, scrub bar, play/pause.

- "Open audio…" -dialogi samalla `FileListDialog`-komponentilla kuin Open
  Video (Vaihe 18), listaa äänitiedostot (esim. `.wav`) kansiosta
- Ladattu äänitiedosto soitetaan samaa libmpv-pohjaista
  `video_player.rs`-polkua pitkin kuin video (mpv toistaa äänitiedostoja
  natiivisti ilman videoraitaa) — scrub bar, aika, play/pause toimivat
  identtisesti video-lähteen kanssa
- Auto-match: valinnan yhteydessä etsi samanniminen `.srt` samasta
  kansiosta (sama käytös kuin Vaihe 18:ssa)
- Vaihe 26–27:n tuore nauhoitus latautuu automaattisesti soittimeen
  stopin jälkeen samaa polkua pitkin

**Voit ajaa/testata:** `cargo run -p trango` — Audio-lähteessä "Open
audio…" listaa kansion äänitiedostot, valinta lataa tiedoston soittimeen
ja sitä voi toistaa/seekata scrub barilla kuten videota.

---

## Vaihe 29 — "Generate subtitles" koko äänitiedostolle

**Tavoite:** Kun äänitiedosto (nauhoitettu tai avattu) on ladattu
Audio-lähteeseen, sille voi generoida tekstityksen yhdellä napilla — koko
tiedosto kerralla, ei per-segmentti kuten alkuperäisessä Vaihe 28:ssa.
Sama Idle/Generating/Done/Error-tilakone kuin Vaihe 20/21.5:ssä videon
puolella; ei uutta rajapintaa, vain uusi kutsupaikka.

- "Generate subtitles" käytettävissä Audio-lähteessä heti kun
  äänitiedosto on ladattu (sama Open Subtitles -dialogin nappi kuin
  video-lähteessä, jos se sopii sellaisenaan — päätä tässä vaiheessa)
- `WhisperCliGenerator` ajetaan suoraan äänitiedostolle ilman
  `extract_audio`-välivaihetta, koska lähde on jo pelkkää ääntä
- Tulos asettaa `PlayerState.cues` samalla tavalla kuin video-lähteen
  generointi

**Voit ajaa/testata:** `cargo run -p trango` — Audio-lähteessä ladatulle
äänitiedostolle "Generate subtitles" tuottaa `.srt`:n taustalla, tila
näkyy Generating→Done, sentence list täyttyy generoinnin valmistuttua.

---

## Vaihe 30 — Validointi: sentence list ja Ctrl+A Audio-lähteessä

**Tavoite:** Varmista ja dokumentoi (ei uutta toiminnallisuutta
odotettavasti) että kaikki cue-pohjaiset ominaisuudet — sentence list,
Ctrl+A-sana-analyysi, käännöstoggle — toimivat identtisesti Audio-
lähteessä kuin Video-lähteessä, koska ne eivät koskaan riippuneet
videosta.

- E2E-testi (kuten Vaihe 13): lataa/generoi cuet Audio-lähteessä, aja
  navigointi + Ctrl+A-analyysi läpi samalla tavalla kuin Video-lähteessä
- Jos jokin osa yllättäen riippuukin videon olemassaolosta (esim. joku
  `Option`-purku joka olettaa videon ladatuksi), korjaa se tässä vaiheessa

**Voit ajaa/testata:** `scripts/test.sh` — uusi E2E-testi vahvistaa cue-
pohjaisten ominaisuuksien toimivan Audio-lähteessä; manuaalinen läpikäynti:
sentence list scrollautuu/korostaa, Ctrl+A avaa analyysin, käännöstoggle
toimii jos käännösanalyysi on ajettu.

---

## Vaihe 31 — Sana-tason audion haarukointi (whisper-cli DTW)

**Tavoite:** Selvittää ohjelmallisesti, missä kohdassa lauseen jo
tunnettua `[start, end)`-aikaväliä kukin sana alkaa ja loppuu — pohjatyö
myöhemmälle automaattiselle ääntämisharjoitteluäänitteelle (käännös +
TTS + nopeusvariantit per sana, koko lause lopuksi — **ei** tässä
vaiheessa, vain sanojen ajoitus). Kirjastotason kyvykkyys ilman UI-
kytkentää tässä vaiheessa.

- `subtitle::WhisperCliWordSegmenter::segment_words` (uusi
  `crates/subtitle/src/word_timing.rs`): leikkaa cuen aikavälin
  `ffmpeg`illä omaksi WAV-pätkäksi ja ajaa sille `whisper-cli`in
  `-ml 1 -sow` (yksi sana per SRT-cue) sekä, jos malli tunnistetaan,
  `-dtw <preset>` tarkempaa sana-tason ajoitusta varten — palauttaa
  `Vec<WordTiming>` (sana + start/end, offsetoitu lauseen omaan alkuun)
- `subtitle::dtw_preset_for_model` päättelee `-dtw`-presetin mallin
  tiedostonimestä (esim. `ggml-large-v3.bin` → `"large.v3"`); `None`
  tuntemattomalle nimelle — `whisper-cli` kaatuu kovaan virheeseen
  väärällä presetillä, joten arvausta vältetään
- Ei UI-kytkentää, ei pysyvyyttä (cache/tallennus) tässä vaiheessa —
  käännös/TTS/nopeusvariantit/harjoitteluäänitteen kokoaminen ovat omia
  myöhempiä vaiheitaan

**Voit ajaa/testata:** `scripts/test.sh` — uudet yksikkötestit
(fake ffmpeg/whisper-cli, ei riipu asennetuista binääreistä) kattavat
`-ss`/`-to`-leikkauksen, `-dtw`-lipun läsnä/poissaolon,
SRT→`WordTiming`-mapin offsetteineen ja temp-tiedostojen siivouksen.
Manuaalinen tarkistus (ei CI:ssä): aja `segment_words` oikealla
lyhyellä pätkällä ja asennetulla mallilla, tarkista silmämääräisesti
että sanat/ajat osuvat `[start, end)`:n sisään ja täsmäävät ääneen.

---

## Ei tässä listassa (myöhempää harkintaa)

- Kansion vaihto Open Video -dialogissa natiivilla kansiovalitsimella
- Oikea on-device STT-toteutus (Vaihe 20 on vain rajapinta + stub)
- Video/tiedostotyyppi-ikonien lopulliset assetit (README: "source real icons... when implementing")
- Pelkän ruudunkaappaus-/pikselivertailu-automaation rakentaminen (Vaihe 22 tehdään manuaalisesti toistaiseksi)
- YouTuben videon suora lataus/toisto (esim. `yt-dlp` + mpv:n `ytdl_hook`) —
  harkittu ja hylätty tekijänoikeussyistä (ks. Vaihe 25 tausta)
- YouTuben valmiiden tekstitysten kaappaus (`yt-dlp --write-auto-sub
  --skip-download`) — harkittu, ei valittu; järjestelmä-audion kaappaus
  (Vaiheet 26–30) valittiin sen sijaan, koska se toimii kaikille
  äänilähteille eikä riipu YouTube-spesifisestä rajapinnasta
- Live/per-segmentti-tekstitysgenerointi VAD-segmentoinnin varassa
  (alkuperäiset Vaiheet 27–28, versiot 0.1.53–0.1.54, `webrtc-vad`) —
  toteutettu ja sittemmin purettu liian monimutkaisena; korvattu
  suoraviivaisemmalla nauhoita-koko-tiedosto-ja-generoi-erikseen-mallilla
  (ks. Vaihe 25:n "Suunnanmuutos"-huomautus yllä)
