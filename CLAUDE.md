# CLAUDE.md — trango

Ohjeet AI-avustajalle tämän projektin kehitystyöhön. Lue myös [README.md](README.md) (handoff-spec: näkymät, tilat, design-tokenit).

## Kieli
- Koodi, identifierit, Rustdoc-kommentit (`///`) ja inline-kommentit (`//`) — kaikki englanniksi
- Keskustelu käyttäjän kanssa — suomeksi

## Projekti
Rust + [Slint](https://slint.dev) + `libmpv`-pohjainen kielenoppimisvideosoitin. Normaali toisto sekä lause-kerrallaan-tila subtitle-cuejen ajastuksen mukaan. Ks. [README.md](README.md) täysi speksi (näkymät, tilat, interaktiot, design-tokenit).

## Keskeiset dokumentit
- [README.md](README.md) — handoff-spec
- CLAUDE.md — tämä tiedosto
- [releasenotes.md](releasenotes.md) — julkaisukohtaiset muutokset (Keep a Changelog -formaatti)
- `docs/` — mdbook: arkkitehtuuri, speksit, käyttö, teknologiaviitteet
- `sketch/design_reference.dc.html` — visuaalinen design-referenssi (ei tuotantokoodia, ei porttata suoraan — ks. README "About the Design Files")

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
- Jokaisen toimivan vaiheen jälkeen: **commit + push**
- Ennen committia: `scripts/check.sh` ja `scripts/test.sh` molempien pitää olla `OK`
- Commit-viesti kuvaa mitä ominaisuutta vaihe koskee

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
```
docs/
  src/
    SUMMARY.md
    usage/           ← asennus, käyttö, näppäinkomennot
    architecture/     ← crate-rakenne, tilamalli, subtitle-pipeline
    specs/            ← toiminnalliset määrittelyt (mitä sovellus tekee)
    technology/       ← yksi sivu per uusi riippuvuus (slint, libmpv, ...)
```
- Uusi crate `Cargo.toml`:iin → oma sivu `docs/src/technology/` + lisäys `SUMMARY.md`:hen (yleiskuvaus, miksi tarvitaan, miksi juuri tämä, käyttöesimerkit projektissa, sudenkuopat)
- Kuvaa nykytila, ei tavoitetta — älä dokumentoi asioita jotka eivät vielä ole totta
- Päivitetään jokaisen koodimuutoksen yhteydessä joka siihen vaikuttaa

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
