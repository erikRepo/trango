# TODO.md — Kehitysaskeleet

Tämä lista pilkkoo trango-projektin pieniin, itsenäisiin askeliin. Jokainen askel
tuottaa jotain **ajettavaa ja testattavaa** — ei koskaan puolivalmista väliaskelta.
Askeleet edetään järjestyksessä ylhäältä alas; älä hyppää eteenpäin ennen kuin
edellinen on "valmis"-kriteerien mukainen.

Katso [CLAUDE.md](CLAUDE.md) kehitystavasta (TDD, skriptit, git-työnkulku,
Rust-konventiot) ja [README.md](README.md) täydestä tuotespeksistä (näkymät,
tilat, design-tokenit).

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

- [ ] Varmista työkalut asennettuna: `rustc`, `cargo`, `mdbook`, `cargo-audit`,
      `cargo-outdated`, ja `libmpv`-kehitysheaderit (esim. `libmpv-dev` /
      `mpv-libs-devel` järjestelmästä riippuen) — `libmpv-rs` tarvitsee nämä
      linkitykseen
- [ ] Päätä minimi Rust-versio (MSRV) ja kirjaa se `Cargo.toml`:iin (`rust-version`)

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

**Voit ajaa/testata:** `cargo run -p trango -- video.mp4 subs.srt` — Normal-moodissa toisto jatkuu keskeytyksettä, hint bar näyttää oikean sisällön moodin mukaan.

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

## Ei tässä listassa (myöhempää harkintaa)

- Kansion vaihto Open Video -dialogissa natiivilla kansiovalitsimella
- Oikea on-device STT-toteutus (Vaihe 20 on vain rajapinta + stub)
- Video/tiedostotyyppi-ikonien lopulliset assetit (README: "source real icons... when implementing")
- Pelkän ruudunkaappaus-/pikselivertailu-automaation rakentaminen (Vaihe 22 tehdään manuaalisesti toistaiseksi)
