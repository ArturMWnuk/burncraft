//! Testy kompilatora G-code (ADR 0011): compile(Job), niezależność od transportu, M3/M4,
//! kompresja, raster S na piksel, M73, dwa lasery, air assist, szablony, blok jednostek,
//! makra, tryby startu. Semantykę wyniku sprawdza interpreter `gcode::sim`.

mod common;

use bc_doc::{AirCode, GcodeUnits, JobOrigin, LayerMode, StartFrom};
use bc_drivers::gcode::sim::{Event, Spindle};
use bc_drivers::{compile, CompileError, Context, DRIVERS};
use bc_geom::Point;
use bc_plan::raster::{bitmap_op, pixel_power, Bitmap, RasterParams};
use bc_plan::Job;
use common::*;

#[test]
fn req_13_015_compile_job_offline_export() {
    let d = rect_doc();
    let j = job(&d);
    let f = compile("grbl", &j, &Context::from_document(&d)).unwrap();
    assert_eq!(f.driver, "grbl");
    assert_eq!(f.extension, "gcode");
    assert!(f.bytes.is_ascii(), "G-code w ASCII");
    let text = f.text().to_string();
    assert!(text.ends_with('\n'));
    assert_eq!(f.lines, text.lines().count());
    assert_eq!(f.hash.len(), 64);
    // czas z modelu przyspieszeń planera
    let est = bc_plan::estimate(&j, &d.plan.kinematics).total_s;
    assert!(close(f.time_s, est, 1e-6), "{} vs {est}", f.time_s);
    // nazwa dokumentu w nagłówku (ASCII)
    assert!(text.contains("Proba"), "{text}");
    // wykonanie: długość z wiązką = długość cięcia planera
    let r = run(&text);
    assert!(close(burn_len(&r), j.stats().cut_length, 1e-3));
    // ogólny CNC
    let g = compile("gcode", &j, &Context::from_document(&d)).unwrap();
    assert_eq!(g.extension, "nc");
    assert_ne!(g.bytes, f.bytes);
    assert!(DRIVERS.contains(&"grbl") && DRIVERS.contains(&"gcode"));
    // nieobsługiwany sterownik
    match compile("ruida", &j, &Context::from_document(&d)) {
        Err(CompileError::UnsupportedDriver(m)) => assert!(m.contains("M3"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        compile("xyz", &j, &Context::default()),
        Err(CompileError::UnsupportedDriver(_))
    ));
    // błędne ustawienia
    let mut ctx = Context::from_document(&d);
    ctx.settings.s_max = 0.0;
    assert!(matches!(
        compile("grbl", &j, &ctx),
        Err(CompileError::Invalid(_))
    ));
    // pusty dokument: plik bez ruchów, z nagłówkiem i stopką
    let e = compile("grbl", &Job::empty(Point::ORIGIN), &Context::default()).unwrap();
    assert!(e.text().contains("M5") && e.text().contains("M2"));
}

#[test]
fn req_13_016_compiler_independent_of_transport() {
    let d = rect_doc();
    let j = job(&d);
    let ctx = Context::from_document(&d);
    let a = compile("grbl", &j, &ctx).unwrap();
    // czysta funkcja: te same wejścia → te same bajty, także w innym wątku
    let (j2, c2) = (j.clone(), ctx.clone());
    let b = std::thread::spawn(move || compile("grbl", &j2, &c2).unwrap())
        .join()
        .unwrap();
    assert_eq!(a.bytes, b.bytes);
    assert_eq!(a.hash, b.hash);
    // strumień wierszy (dowolny transport) = plik
    let mut joined: String = a.stream_lines().collect::<Vec<_>>().join("\n");
    joined.push('\n');
    assert_eq!(joined.as_bytes(), a.bytes.as_slice());
    for l in a.stream_lines() {
        assert!(!l.contains('\r'));
        // wiersz mieści się w buforze RX GRBL (127 B) z nową linią
        assert!(l.len() < 127, "{l}");
    }
    // postęp i anulowanie bez wejścia/wyjścia
    let mut calls = 0;
    bc_drivers::compile_with_progress("grbl", &j, &ctx, &mut |_, _| {
        calls += 1;
        Ok(())
    })
    .unwrap();
    assert!(calls >= 2);
    let r = bc_drivers::compile_with_progress("grbl", &j, &ctx, &mut |_, _| {
        Err(CompileError::Cancelled)
    });
    assert_eq!(r, Err(CompileError::Cancelled));
}

#[test]
fn req_13_085_grbl_m3_m4() {
    let mut d = rect_doc();
    // M4 (moc dynamiczna) — domyślnie
    let f = gc(&d, "grbl");
    let t = f.text();
    assert!(t.contains("M4 S0"), "{t}");
    assert!(!t.lines().any(|l| l.starts_with("M3")), "{t}");
    let r = run(t);
    let b = burns(&r);
    assert!(!b.is_empty());
    assert!(b
        .iter()
        .all(|m| m.spindle == Spindle::M4 && !m.rapid && m.s == 500.0));
    assert!(r.moves().any(|m| m.rapid), "przejazd G0 bez wiązki");
    // wiązka wyłączona po ostatnim cięciu
    let tl: Vec<&str> = t.lines().collect();
    let last_cut = tl
        .iter()
        .rposition(|l| l.starts_with('X') || l.starts_with('Y'))
        .unwrap();
    assert!(tl[last_cut..].contains(&"M5"), "{t}");
    // M3 (moc stała)
    d.layer_mut(0).unwrap().params.dynamic_power = false;
    let t = gc(&d, "grbl").text().to_string();
    assert!(t.contains("M3 S0") && !t.contains("M4"), "{t}");
    assert!(burns(&run(&t)).iter().all(|m| m.spindle == Spindle::M3));
    // ogólny CNC: zawsze M3, M5 przed każdym przejazdem (bez trybu laserowego)
    d.layer_mut(0).unwrap().params.dynamic_power = true;
    let t = gc(&d, "gcode").text().to_string();
    assert!(!t.contains("M4"), "{t}");
    let r = run_with(&t, false);
    assert!(r.moves().filter(|m| m.rapid).all(|m| !m.on), "{t}");
    assert!(close(burn_len(&r), 100.0, 1e-3));
}

#[test]
fn req_13_086_gcode_compression() {
    let mut d = rect_doc();
    d.add(0, rect(0.5, 50.0, 20.0, 10.0)).unwrap();
    let c = gc(&d, "grbl");
    d.gcode.compress = false;
    let u = gc(&d, "grbl");
    assert!(
        c.bytes.len() < u.bytes.len(),
        "{} vs {}",
        c.bytes.len(),
        u.bytes.len()
    );
    // ten sam tor po wykonaniu
    let (rc, ru) = (run(c.text()), run(u.text()));
    let (mc, mu): (Vec<_>, Vec<_>) = (rc.moves().collect(), ru.moves().collect());
    assert_eq!(mc.len(), mu.len());
    for (a, b) in mc.iter().zip(&mu) {
        assert!(pclose(a.from, b.from, 1e-9) && pclose(a.to, b.to, 1e-9));
        assert_eq!((a.on, a.s, a.feed, a.rapid), (b.on, b.s, b.feed, b.rapid));
    }
    let ct = c.text();
    // pomijanie powtórzeń: F i S raz, G1 tylko po G0
    assert_eq!(ct.matches("F600").count(), 1, "{ct}");
    assert_eq!(ct.matches("S500").count(), 1, "{ct}");
    let motion: Vec<&str> = ct
        .lines()
        .filter(|l| l.starts_with('G') || l.starts_with('X') || l.starts_with('Y'))
        .collect();
    for w in motion.windows(2) {
        assert!(
            !(w[0].starts_with("G1") && w[1].starts_with("G1 ")),
            "powtórzone G1: {w:?}"
        );
    }
    // skrócone współrzędne i pominięte osie bez zmiany
    assert!(ct.contains("X.5"), "{ct}");
    assert!(
        ct.lines().any(|l| l == "X10") && ct.lines().any(|l| l == "Y30"),
        "{ct}"
    );
    let ut = u.text();
    assert!(ut.contains("X0.500"), "{ut}");
    for l in ut.lines().filter(|l| l.starts_with("G1")) {
        for w in ['X', 'Y', 'S', 'F'] {
            assert!(l.contains(w), "bez kompresji pełne słowa: {l}");
        }
    }
}

#[test]
fn req_13_087_raster_s_per_pixel() {
    // syntetyczna bitmapa 8×4: gradient, biała dziura, czarny piksel
    let (w, h) = (8u32, 4u32);
    let mut data = Vec::new();
    for r in 0..h {
        for c in 0..w {
            data.push(match (r, c) {
                (1, 3) => 255,
                (2, 5) => 0,
                _ => (c * 30 + r * 7) as u8,
            });
        }
    }
    let b = Bitmap {
        width: w,
        height: h,
        data,
        origin: Point::new(5.0, 5.0),
        pixel: 0.5,
    };
    let p = RasterParams {
        speed: 100.0,
        power_min: 10.0,
        power_max: 80.0,
        bidirectional: true,
        overscan: 1.0,
    };
    let op = bitmap_op(0, "obraz", &b, &p, true);
    assert_eq!(op.mode, LayerMode::Image);
    let j = Job {
        ops: vec![op],
        ..Job::empty(Point::ORIGIN)
    };
    let f = compile("grbl", &j, &Context::default()).unwrap();
    let r = run(f.text());
    for row in 0..h {
        for col in 0..w {
            let c = b.center(col, row);
            let want = pixel_power(b.at(col, row), &p) * 1000.0 / 100.0;
            let got = r.burn_at(c, 1e-6);
            if want == 0.0 {
                assert_eq!(got, None, "piksel {col},{row} biały");
            } else {
                let got = got.unwrap_or_else(|| panic!("piksel {col},{row} bez wiązki"));
                assert!(
                    close(got, want, 0.006),
                    "piksel {col},{row}: S {got} ≠ {want}"
                );
            }
        }
    }
    // overscan bez wiązki, wiersze w obu kierunkach
    let idle: Vec<_> = r.moves().filter(|m| !m.rapid && !m.on).collect();
    assert!(!idle.is_empty());
    let dirs: Vec<bool> = burns(&r)
        .windows(1)
        .map(|m| m[0].to.x > m[0].from.x)
        .collect();
    assert!(dirs.contains(&true) && dirs.contains(&false));
}

#[test]
fn req_13_088_m73_progress() {
    let mut d = rect_doc();
    for i in 0..5 {
        d.add(0, rect(50.0 + i as f64 * 25.0, 10.0, 20.0, 20.0))
            .unwrap();
    }
    assert!(
        !gc(&d, "grbl").text().contains("M73"),
        "domyślnie wyłączone"
    );
    d.gcode.progress_m73 = true;
    let f = gc(&d, "grbl");
    let r = run(f.text());
    let pr: Vec<(f64, Option<f64>)> = r
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Progress {
                percent, minutes, ..
            } => Some((*percent, *minutes)),
            _ => None,
        })
        .collect();
    assert!(pr.len() > 5, "{pr:?}");
    assert_eq!(pr.first().unwrap().0, 0.0);
    assert_eq!(*pr.last().unwrap(), (100.0, Some(0.0)));
    for w in pr.windows(2) {
        assert!(w[1].0 > w[0].0, "procent rośnie: {pr:?}");
        assert!(w[1].1.unwrap() <= w[0].1.unwrap(), "pozostały czas maleje");
    }
    // M73 przed pierwszym ruchem i po ostatnim
    let t = f.text();
    let first = line_idx(t, "M73").unwrap();
    assert!(first < line_idx(t, "G0").unwrap());
    let tl: Vec<&str> = t.lines().collect();
    let last_m73 = tl.iter().rposition(|l| l.starts_with("M73")).unwrap();
    let last_g1 = tl
        .iter()
        .rposition(|l| l.starts_with('X') || l.starts_with('Y') || l.starts_with("G1"))
        .unwrap();
    assert!(last_m73 > last_g1);
}

#[test]
fn req_13_089_two_lasers() {
    let mut d = rect_doc();
    d.layer_mut(0).unwrap().power_max = 60.0;
    d.layer_mut(0).unwrap().params.power2_max = Some(30.0);
    // drugi laser wyłączony: ostrzeżenie, tylko laser 1
    let f = gc(&d, "grbl");
    assert!(f.warnings.iter().any(|w| w.code == "laser2_disabled"));
    assert!(!f.text().contains("T1"));
    d.gcode.laser2.enabled = true;
    d.gcode.laser2.offset = [5.0, -2.0];
    let f = gc(&d, "grbl");
    let t = f.text();
    let (t0, t1) = (line_idx(t, "T0").unwrap(), line_idx(t, "T1").unwrap());
    assert!(t0 < t1);
    let r = run(t);
    let l1: Vec<_> = burns(&r).into_iter().filter(|m| m.tool == 0).collect();
    let l2: Vec<_> = burns(&r).into_iter().filter(|m| m.tool == 1).collect();
    assert!(!l1.is_empty() && !l2.is_empty());
    assert!(l1.iter().all(|m| m.s == 600.0));
    assert!(l2.iter().all(|m| m.s == 300.0), "moc lasera 2 = power2_max");
    // laser 2 trafia w te same miejsca: głowica przesunięta o −offset
    for (a, b) in l1.iter().zip(&l2) {
        assert!(pclose(a.to, b.to + bc_geom::Vec2::new(5.0, -2.0), 1e-9));
    }
    assert!(close(burn_len(&r), 200.0, 1e-3));
    // tylko laser 2 (laser 1 bez mocy)
    d.layer_mut(0).unwrap().power_max = 0.0;
    let r = run(gc(&d, "grbl").text());
    assert!(burns(&r).iter().all(|m| m.tool == 1));
}

#[test]
fn req_13_090_air_assist() {
    let mut d = rect_doc();
    d.layer_mut(0).unwrap().air_assist = true;
    d.add(1, rect(60., 10., 20., 20.)).unwrap();
    d.layer_mut(1).unwrap().air_assist = false;
    let f = gc(&d, "grbl");
    let t = f.text();
    assert!(line_idx(t, "M8").unwrap() < line_idx(t, "M9").unwrap());
    let r = run(t);
    for m in burns(&r) {
        if m.to.x <= 40.0 + 1e-9 {
            assert!(m.air.flood && !m.air.mist, "warstwa 0 z air: {m:?}");
        } else {
            assert!(!m.air.any(), "warstwa 1 bez air: {m:?}");
        }
    }
    // M7 zamiast M8; air wyłączone na końcu
    d.gcode.air_code = AirCode::M7;
    d.layer_mut(1).unwrap().air_assist = true;
    let t = gc(&d, "grbl").text().to_string();
    assert!(t.contains("M7") && !t.contains("M8"), "{t}");
    let r = run(&t);
    assert!(burns(&r).iter().all(|m| m.air.mist));
    let tl: Vec<&str> = t.lines().collect();
    let last_air = tl.iter().rposition(|l| *l == "M7" || *l == "M9").unwrap();
    assert_eq!(tl[last_air], "M9");
}

#[test]
fn req_13_105_header_footer_templates() {
    let mut d = rect_doc();
    let t = gc(&d, "grbl").text().to_string();
    assert!(
        t.starts_with("; BurnCraft G-code (grbl): Proba\n; Estimated time: 00:00:"),
        "{t}"
    );
    assert!(t.ends_with("M2\n"));
    d.gcode.header =
        "; {name} / {driver} / S{s_max} / {x0},{y0}-{x1},{y1} / {passes} / {nope}".into();
    d.gcode.footer = "M5\n; koniec {name}\nM30".into();
    let f = gc(&d, "grbl");
    let t = f.text();
    assert!(
        t.starts_with("; Proba / grbl / S1000 / 10,10-40,30 / 1 / {nope}\n"),
        "{t}"
    );
    assert!(t.ends_with("M5\n; koniec Proba\nM30\n"), "{t}");
    assert!(f
        .warnings
        .iter()
        .any(|w| w.code == "unknown_placeholder" && w.message.contains("{nope}")));
    // ogólny CNC: komentarze w nawiasach
    let t = gc(&d, "gcode").text().to_string();
    assert!(t.starts_with("(Proba / gcode /"), "{t}");
    assert!(t.contains("(koniec Proba)"), "{t}");
}

#[test]
fn req_13_106_units_block() {
    let mut d = rect_doc();
    let t = gc(&d, "grbl").text().to_string();
    let g0 = line_idx(&t, "G0").unwrap();
    assert!(line_idx(&t, "G90").unwrap() < g0 && line_idx(&t, "G21").unwrap() < g0);
    // cale: G20, współrzędne i posuw w calach
    d.gcode.units = GcodeUnits::Inch;
    d.gcode.decimals = 4;
    let t = gc(&d, "grbl").text().to_string();
    assert!(t.contains("G20") && !t.contains("G21"), "{t}");
    assert!(t.contains("X.3937"), "{t}");
    assert!(t.contains("F23.62"), "{t}");
    let r = run(&t);
    assert!(close(burn_len(&r), 100.0, 0.01));
    let b = burns(&r);
    assert!(b.iter().all(|m| close(m.feed, 600.0, 0.1)));
    // własny blok wstawiany dosłownie
    d.gcode.units = GcodeUnits::Mm;
    d.gcode.units_block = "G21 G90 G94\nG17".into();
    let t = gc(&d, "grbl").text().to_string();
    assert!(t.contains("\nG21 G90 G94\nG17\n"), "{t}");
    assert_eq!(lines_starting(&t, "G90"), 0);
    assert_eq!(lines_starting(&t, "G21"), 1);
}

#[test]
fn req_13_107_layer_macros_pre_post() {
    let mut d = rect_doc();
    d.layer_mut(0).unwrap().passes = 2;
    d.layer_mut(0).unwrap().params.gcode_pre =
        "; pre {label} S{power} F{speed} {power_pct}%".into();
    d.layer_mut(0).unwrap().params.gcode_post = "; post {layer} pass {pass}".into();
    d.add(1, rect(60., 10., 20., 20.)).unwrap();
    d.layer_mut(1).unwrap().params.gcode_pre = "M7".into();
    let t = gc(&d, "grbl").text().to_string();
    let lines: Vec<&str> = t.lines().collect();
    let pre = lines.iter().position(|l| l.starts_with("; pre")).unwrap();
    assert_eq!(lines[pre], "; pre C00 S500 F600 50%");
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("; pre")).count(),
        1,
        "raz na warstwę"
    );
    let post = lines.iter().position(|l| l.starts_with("; post")).unwrap();
    assert_eq!(lines[post], "; post 0 pass 2");
    let pass1 = lines.iter().position(|l| l.contains("pass 1")).unwrap();
    let pass2 = lines
        .iter()
        .position(|l| l.starts_with("; C00 - pass 2"))
        .unwrap();
    let m7 = lines.iter().position(|l| *l == "M7").unwrap();
    assert!(pre < pass1 && pass2 < post && post < m7, "{t}");
    // makro po ostatnim ruchu warstwy 0
    assert!(lines[pass2..post]
        .iter()
        .any(|l| l.starts_with('X') || l.starts_with('Y')));
}

#[test]
fn req_13_108_job_macros_pre_post() {
    let mut d = rect_doc();
    d.gcode.job_pre = "; przed {name}\nM8".into();
    d.gcode.job_post = "M9\n; po".into();
    let t = gc(&d, "grbl").text().to_string();
    let l: Vec<&str> = t.lines().collect();
    let pre = l.iter().position(|x| *x == "; przed Proba").unwrap();
    let g21 = l.iter().position(|x| *x == "G21").unwrap();
    let first_op = l.iter().position(|x| x.starts_with("; C00")).unwrap();
    let post = l.iter().position(|x| *x == "; po").unwrap();
    let last_move = l
        .iter()
        .rposition(|x| x.starts_with('X') || x.starts_with('Y') || x.starts_with("G1"))
        .unwrap();
    let footer = l.iter().rposition(|x| *x == "M2").unwrap();
    assert!(g21 < pre && pre < first_op, "{t}");
    assert!(last_move < post && post < footer, "{t}");
    assert_eq!(l[pre + 1], "M8");
    // wiązka wyłączona po makrze zadania (bezpieczeństwo)
    assert!(l[post..footer].contains(&"M5"));
}

#[test]
fn req_10_006_start_from_modes_in_output() {
    let mut d = rect_doc();
    // absolute: współrzędne obszaru roboczego
    let r = run(gc(&d, "grbl").text());
    let b = burns(&r);
    assert!(b.iter().any(|m| pclose(m.to, Point::new(40., 30.), 1e-9)));
    // zero użytkownika, origin = środek ramki → współrzędne względem (25, 20)
    d.start.from = StartFrom::UserOrigin;
    d.start.origin = JobOrigin::Center;
    let t = gc(&d, "grbl").text().to_string();
    assert!(!t.contains("G91"));
    let b = burns(&run(&t));
    assert!(
        b.iter().any(|m| pclose(m.to, Point::new(15., 10.), 1e-9)),
        "{t}"
    );
    assert!(
        b.iter().any(|m| pclose(m.to, Point::new(-15., -10.), 1e-9)),
        "{t}"
    );
    // bieżąca pozycja: G91, start w punkcie origin (lewy dolny róg), powrót do startu
    d.start.from = StartFrom::Current;
    d.start.origin = JobOrigin::BottomLeft;
    let t = gc(&d, "grbl").text().to_string();
    assert!(t.contains("\nG91\n"), "{t}");
    let r = run(&t);
    let b = burns(&r);
    assert!(
        b.iter().any(|m| pclose(m.to, Point::new(30., 20.), 1e-9)),
        "{t}"
    );
    assert!(close(burn_len(&r), 100.0, 1e-6));
    let last = r.moves().last().unwrap();
    assert!(
        pclose(last.to, Point::ORIGIN, 1e-9),
        "powrót do punktu startu"
    );
    // przyrosty z zaokrąglonych pozycji: brak narastania błędu na wielu ruchach
    let mut d2 = rect_doc();
    for i in 0..200 {
        d2.add(0, rect(1.0 + i as f64 * 0.3333, 50.0, 0.1111, 0.2222))
            .unwrap();
    }
    d2.start.from = StartFrom::Current;
    let r = run(gc(&d2, "grbl").text());
    assert!(pclose(r.moves().last().unwrap().to, Point::ORIGIN, 1e-9));
}
