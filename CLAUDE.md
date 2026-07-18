# CLAUDE.md — trango

Ohjeet AI-avustajalle tämän projektin kehitystyöhön. Lue myös [SPEC.md](SPEC.md) (handoff-spec: näkymät, tilat) ja [STYLE.md](STYLE.md) (design-tokenit).

## Kieli
- Koodi, identifierit, Rustdoc-kommentit (`///`) ja inline-kommentit (`//`) — kaikki englanniksi
- Keskustelu käyttäjän kanssa — suomeksi

## Projekti
Rust + [Slint](https://slint.dev) + `libmpv`-pohjainen kielenoppimisvideosoitin. Normaali toisto sekä lause-kerrallaan-tila subtitle-cuejen ajastuksen mukaan. Ks. [SPEC.md](SPEC.md) täysi speksi (näkymät, tilat, interaktiot) ja [STYLE.md](STYLE.md) (design-tokenit).

## Keskeiset dokumentit
- [README.md](README.md) — lyhyt projektikuvaus + linkki mdbook-dokumentaatioon
- [SPEC.md](SPEC.md) — alkuperäinen handoff-spec (näkymät, tilat, interaktiot)
- [STYLE.md](STYLE.md) — visuaalinen design-referenssi ja design-tokenit
- CLAUDE.md — tämä tiedosto
- [releasenotes.md](releasenotes.md) — julkaisukohtaiset muutokset (Keep a Changelog -formaatti)
- `docs/` — mdbook (käyttäjädokumentaatio + kehittäjäopas): asennus, käyttö, arkkitehtuuri, speksit, teknologiaviitteet
- `sketch/design_reference.dc.html` — visuaalinen design-referenssi (ei tuotantokoodia, ei porttata suoraan — ks. STYLE.md "About the Design Files")

## Kehitystapa: TDD
- Jokainen ominaisuus tehdään red → green → refactor -syklillä: testi ensin, sitten toteutus, sitten siivous
- Yksi ominaisuus = yksi pieni, itsenäinen kokonaisuus — ei useita ominaisuuksia samassa vaiheessa
- Ei toteutusta ilman epäonnistuvaa testiä ensin

## Testien ajaminen — token-säästö
- **Älä aja `cargo test` suoraan.** Käytä `scripts/test.sh`.
- Skripti ajaa koko testisuite hiljaisesti ja tulostaa vain `OK` tai `FAIL` + lokin
- Testin tarkempi tarkastelu (koodi, assertit, stack trace) tehdään **vain jos** `scripts/test.sh` palauttaa `FAIL` — muuten tuloste riittää

## Koodin tyylittely — joka vaiheessa
- **Älä aja `cargo fmt` / `cargo clippy` suoraan.** Käytä `scripts/check.sh`.
- Ajetaan jokaisen kehitysvaiheen jälkeen, ennen committia
- Tulostaa vain `OK` tai `FAIL` + lokin — token-säästö sama periaate kuin testeissä

## Riippuvuuksien tarkistus (satunnaisesti, ei joka vaiheessa)
- `scripts/deps-check.sh` → `cargo audit` + `cargo outdated`, ajetaan esim. ennen isompaa julkaisua tai kun uutta riippuvuutta harkitaan
- Kysy aina käyttäjältä ennen uuden riippuvuuden lisäämistä ja perustele miksi se tarvitaan
- Käytä aina uusinta vakaata versiota

## Git-työnkulku
- GitHubissa pull requestit ovat käytössä — **ei suoraa committia/pushia `master`-branchiin**
- Jokaista toimivaa vaihetta varten: oma feature-branch kuvaavalla nimellä (esim. `normal-mode-hint-bar-and-scrub-seek`), commit + push sille branchille, sitten `gh pr create` masteria vasten
- Ennen committia: `scripts/check.sh` ja `scripts/test.sh` molempien pitää olla `OK`
- Commit-viesti kuvaa mitä ominaisuutta vaihe koskee
- PR:n kuvaus (`gh pr create --body`) kertaa lyhyesti mitä muuttui ja miksi — ei tarvitse toistaa commit-viestejä sanasta sanaan
- PR:n mergeaa käyttäjä GitHubissa (tai pyytää erikseen); älä mergeä tai pushita suoraan masteriin ilman lupaa

## Versiointi
- `Cargo.toml`-versio kasvatetaan jokaisen vaiheen/committin yhteydessä (patch-taso, esim. `0.1.0` → `0.1.1`)
- Versionumero näkyy myös UI:ssa (esim. top bar / ikkunan otsikko) — päivitä UI:n versionäyttö samassa committissa kuin `Cargo.toml`-bump
- [releasenotes.md](releasenotes.md) päivitetään joka versiolle Keep a Changelog -formaatilla: `### Added`, `### Changed`, `### Fixed`, `### Removed`
- `[Unreleased]`-osio pysyy aina releasenotes.md:n yläosassa seuraavaa versiota varten

## E2E-testit
- Aloitetaan heti ensimmäisten ominaisuuksien valmistuttua — ei jätetä loppuun
- Testataan oikealla Slint-UI:lla / libmpv-integraatiolla siinä laajuudessa kuin käytännössä mahdollista (subtitle-parsintaa oikeilla tiedostoilla, cue-navigointia, tilamuutoksia) — ei pelkkiä eristettyjä yksikkötestejä

## Rust-konventiot
- **Error handling:** `thiserror` kirjastokoodissa (crate-kohtaiset virhetyypit), `anyhow` binäärissä ja integraatiotesteissä
- Ei `unwrap()` / `expect()` muualla kuin testeissä
- **Logging:** `tracing`-crate — ei `println!`-tulostuksia tuotantokoodissa (tasot: `trace`/`debug`/`info`/`warn`/`error`)
- **Käyttäjän asetukset: CLI-vipu tai `config.toml`, ei ympäristömuuttuja.** Esim. debug-lokitus on `--debug`-vipu (`main.rs`:n `extract_debug_flag`), ei `RUST_LOG`; mallivalinnat ja muut pysyvät asetukset menevät `config.rs`:n kautta `config.toml`:iin (ks. `crates/app/src/config.rs`). Ympäristömuuttuja on hyväksyttävä vain kun kyse on aidosti kertaluontoisesta, harvoin muuttuvasta järjestelmätason polusta (esim. `TRANGO_WHISPER_CLI_PATH`/`TRANGO_FFMPEG_PATH`/`TRANGO_NIQUD_CLI_PATH` — ulkoisen binäärin sijainti, ei sovelluksen oma käyttäjäasetus)
- Tiedostot lyhyitä ja yhden vastuun mukaisia — jos tiedosto kasvaa yli ~200 riviä, harkitse jakamista
- Ei dead codea tai placeholder-kommentteja
- Kaikilla funktioilla, structeilla ja enumeilla — julkisilla ja yksityisillä — `///`-rustdoc-kommentti
- Testifunktioille ei rustdoc-kommenttia; sen sijaan kolme `//`-kommenttia funktion alussa:
  ```rust
  #[test]
  fn test_something() {
      // Given: lähtötilanne
      // When:  toiminto
      // Then:  odotettu lopputulos
      ...
  }
  ```
  Hyvin lyhyet, itsestään selvät testit voivat tiivistää tämän yhdelle riville tai jättää pois.

## Dokumentaatio (mdbook, `docs/`)

mdBook on käyttäjälle suunnattu: tarina etenee sivu sivulta yksinkertaisesta
kohti syvempää, developer-sisältö on omana osionaan vasta lopussa.

```
docs/
  src/
    SUMMARY.md
    README.md                  ← etusivu: yleiskuvaus sovelluksesta
    getting-started/           ← asennus, ensimmäisen videon avaaminen
    usage/                     ← ominaisuudet käyttäjälle: tilat, pikanäppäimet,
                                  tekstitykset, asetukset — yksi sivu per aihe
    developer/                 ← kehittäjädokumentaatio, oma yläosio SUMMARY.md:ssä
      README.md                 ← osion etusivu
      architecture/              ← crate-rakenne, tilamalli, video-pipeline
      specs.md                   ← toiminnalliset määrittelyt / päätöslogi
      technology/                ← yksi sivu per riippuvuus (slint, libmpv, ...)
```
- Uusi crate `Cargo.toml`:iin → oma sivu `docs/src/developer/technology/` + lisäys `SUMMARY.md`:hen (yleiskuvaus, miksi tarvitaan, miksi juuri tämä, käyttöesimerkit projektissa, sudenkuopat)
- Uusi käyttäjälle näkyvä ominaisuus → oma tai olemassa olevaa sivua täydentävä sivu `docs/src/usage/`-hakemistoon, kirjoitettuna käyttäjälle (ei toteutusyksityiskohtia) — toteutuksen taustat/päätökset menevät `docs/src/developer/specs.md`:hen
- Kuvaa nykytila, ei tavoitetta — älä dokumentoi asioita jotka eivät vielä ole totta
- Päivitetään jokaisen koodimuutoksen yhteydessä joka siihen vaikuttaa

### Ytimekkyys — pakollinen jokaisella päivityksellä

mdBook kuluttaa tokeneita joka kerta kun sitä luetaan, eikä kukaan jaksa
lukea pitkiä kertomuksia — tiiviys on tärkeämpää kuin kattavuus. Ennen
kuin lisäät tai muokkaat mitä tahansa `docs/`-sivua:

- **Pituusbudjetti (ohjeellinen yläraja, ei tavoite):** `technology/*.md`
  ~20–30 riviä; yksi `specs.md`-päätös ~5–15 riviä; `usage/`-sivu ~30–60
  riviä; `architecture/`-sivu ~60–120 riviä (mekaniikka saa olla
  yksityiskohtaisempi, mutta ei toista samaa asiaa kahdesti). Jos sivu
  kasvaa selvästi yli tämän, karsi ennen committia — älä vain lisää.
- **Lopputulos, ei matka.** Kirjoita mitä päätettiin ja miksi — ei
  kronologiaa epäonnistuneista yrityksistä, debug-sessioista tai "löytyi
  testatessa X, sitten Y, sitten vasta Z toimi" -kerrontaa. Yksi lause
  perustelulle ja yksi keskeiselle sudenkuopalle riittää useimmiten;
  toisen/kolmannen yrityksen historia jätetään kokonaan pois, ellei se
  ole ainoa tapa selittää *miksi* nykyinen ratkaisu on juuri tällainen.
- **Ei koodilohkoja "havainnollistamaan" jotain minkä proosa jo sanoo** —
  vain jos koodi itsessään on se olennainen asia (esim. tarkka
  rajapintasignatuuri). Yksi lyhyt esimerkki per sivu on yleensä kaikki
  mitä tarvitaan.
- **Ei viittauksia `TODO.md`:n "Vaihe"-numeroihin tai muihin
  kehitysvaiheisiin** dokumentaatiossa — ne eivät kestä ajan yli eivätkä
  merkitse mitään lukijalle, jolla ei ole `TODO.md` auki. Kuvaa asia
  suoraan, ilman kehitysjärjestystä.
- Kun muokkaat olemassa olevaa sivua, tarkista samalla ettei se ole
  kasvanut rönsyileväksi — pieni siivous (poista vanhentunut kohta,
  yhdistä toistoa) kuuluu samaan committiin, ei erilliseen "siivous"-vaiheeseen.

## Vaaditut työkalut
```
cargo install cargo-audit
cargo install cargo-outdated
cargo install mdbook
```

## Laatu ennen kuin vaihe on "valmis"
1. `scripts/check.sh` → `OK` (fmt + clippy)
2. `scripts/test.sh` → `OK`
3. Versio kasvatettu `Cargo.toml`:ssa ja näkyy UI:ssa
4. `releasenotes.md` päivitetty
5. commit + push
