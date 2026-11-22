//! Unit tests for Genius lyrics search matching and embed parsing.

use serde_json::Value;

use super::*;

/// Byte for byte as Genius encodes embeds (made by the research encoder
/// that reproduces 105 real embeds); every lyric here is invented.
const PT_BR_HEADER: &str = r#"if (document.getElementById('rg_embed_link_90000001')) {
  document.write('<link rel="stylesheet" media="screen" href="https://assets.genius.com/stylesheets/compiled/embedded_song-e8cb29d87f65b77ac44550874d60bddd.css" />')
  document.write(JSON.parse('\"\\n  <div class=\\\"rg_embed music\\\" id=\\\"rg_embed_90000001\\\" data-domain=\\\"genius.com\\\">\\n    <div class=\\\"rg_embed_header\\\">\\n  <a target=\\\"_blank\\\" class=\\\"rg_embed_logo\\\" href=\\\"https://genius.com/Nora-vale-lua-de-papel-lyrics\\\">Powered by Genius<\/a>\\n<\/div>\\n\\n    <div class=\\\"rg_embed_body\\\">\\n  <p>[Letra de \\\"Lua de Papel]<br>\\n<br>\\n[Intro]<br>\\nAcordei com o mundo de ponta-cabeça<br>\\nE avisou que o dia \'tava lento<br>\\n(Ei, ei, ei—)<br>\\n<br>\\n[Verso 1: Nora Vale]<br>\\nComprei um sonho em \\\"promoção\\\", sem troco<br>\\nDobrei a lua em papel e guardei no bolso<br>\\nD\'água, d\'ouro, d’ali: tudo cabe no meu som<br>\\nRock &amp; Roll de esquina, dia/noite sem horário<br>\\nNinguém sabe o que é ser de lá — nem eu<br>\\n<br>\\n[Refrão]<br>\\nLua de papel, lua de papel<br>\\nAmassa mas não rasga<br>\\n<br>\\n[Verso 2: Nora Vale &amp; Dani Rocha]<br>\\nCoração de lata, açúcar no café<br>\\nFé, fé, fé<\/p>\\n\\n\\n<\/div>\\n\\n    <div class=\\\"rg_embed_footer\\\">\\n  <a target=\\\"_blank\\\" href=\\\"https://genius.com/Nora-vale-lua-de-papel-lyrics\\\">\\n    <div class=\\\"song_title\\\">Lua de Papel<\/div>\\n    Nora Vale\\n<\/a><\/div>\\n\\n  <\/div>\\n<iframe src=\\\"//genius.com/songs/90000001/embed_analytics\\\"\\n              class=\\\"rg_embed_analytics\\\"\\n              id=\\\"rg_embed_analytics_90000001\\\"><\/iframe>\"'))
  document.write('<script src="https://assets.genius.com/javascripts/compiled/embedded_song-f6614cf11067947616d840277d246cdd.js" async="async" crossorigin="true"></script>')
}
"#;
const JS_ESCAPES: &str = r#"if (document.getElementById('rg_embed_link_90000014')) {
  document.write('<link rel="stylesheet" media="screen" href="https://assets.genius.com/stylesheets/compiled/embedded_song-e8cb29d87f65b77ac44550874d60bddd.css" />')
  document.write(JSON.parse('\"\\n  <div class=\\\"rg_embed music\\\" id=\\\"rg_embed_90000014\\\" data-domain=\\\"genius.com\\\">\\n    <div class=\\\"rg_embed_header\\\">\\n  <a target=\\\"_blank\\\" class=\\\"rg_embed_logo\\\" href=\\\"https://genius.com/Nora-vale-dollar-sign-lyrics\\\">Powered by Genius<\/a>\\n<\/div>\\n\\n    <div class=\\\"rg_embed_body\\\">\\n  <p>Ty Dolla \$ign said it\'s 5\$ a ticket<br>\\nBack\`tick and \\\"double\\\" and \'single\' quotes<br>\\nA backslash \\\\ in the middle<br>\\nClose tag lookalike: &lt;/p&gt; stays text<br>\\nPath-ish: a/b/c and https://example.invalid/x<\/p>\\n\\n\\n<\/div>\\n\\n    <div class=\\\"rg_embed_footer\\\">\\n  <a target=\\\"_blank\\\" href=\\\"https://genius.com/Nora-vale-dollar-sign-lyrics\\\">\\n    <div class=\\\"song_title\\\">\$ign of the Times<\/div>\\n    Nora Vale\\n<\/a><\/div>\\n\\n  <\/div>\\n<iframe src=\\\"//genius.com/songs/90000014/embed_analytics\\\"\\n              class=\\\"rg_embed_analytics\\\"\\n              id=\\\"rg_embed_analytics_90000014\\\"><\/iframe>\"'))
  document.write('<script src="https://assets.genius.com/javascripts/compiled/embedded_song-f6614cf11067947616d840277d246cdd.js" async="async" crossorigin="true"></script>')
}
"#;
const JS_UNICODE_ESCAPES: &str = r#"if (document.getElementById('rg_embed_link_90000016')) {
  document.write('<link rel="stylesheet" media="screen" href="https://assets.genius.com/stylesheets/compiled/embedded_song-e8cb29d87f65b77ac44550874d60bddd.css" />')
  document.write(JSON.parse('\"\\n  <div class=\\\"rg_embed music\\\" id=\\\"rg_embed_90000016\\\" data-domain=\\\"genius.com\\\">\\n    <div class=\\\"rg_embed_body\\\">\\n  <p>Caf\u00e9 caf\\u00e9 \\ud83c\\udf19<br>\\n\ud83c\udf19 \x41\x42 slash\/ok<br>\\nend \\\"q\\\"<\/p>\\n\\n\\n<\/div>\\n\\n    <div class=\\\"rg_embed_footer\\\">\\n  <\/div>\"'))
  document.write('<script src="https://assets.genius.com/javascripts/compiled/embedded_song-f6614cf11067947616d840277d246cdd.js" async="async" crossorigin="true"></script>')
}
"#;

/// An embed.js written the way Genius writes one around `body` (the
/// HTML inside `rg_embed_body`): JSON-encoded, then escaped like Rails'
/// `escape_javascript`.
fn embed_js(body: &str) -> String {
    let html = format!(
        concat!(
            "\n  <div class=\"rg_embed music\" id=\"rg_embed_90000099\" data-domain=\"genius.com\">\n",
            "    <div class=\"rg_embed_header\">\n",
            "  <a target=\"_blank\" class=\"rg_embed_logo\" href=\"https://genius.com/Nora-vale-test-lyrics\">Powered by Genius</a>\n",
            "</div>\n\n",
            "    <div class=\"rg_embed_body\">{}</div>\n\n",
            "    <div class=\"rg_embed_footer\">\n",
            "  <a target=\"_blank\" href=\"https://genius.com/Nora-vale-test-lyrics\">\n",
            "    <div class=\"song_title\">Test Song</div>\n",
            "    Nora Vale\n",
            "</a></div>\n\n",
            "  </div>\n",
            "<iframe src=\"//genius.com/songs/90000099/embed_analytics\"\n",
            "              class=\"rg_embed_analytics\"\n",
            "              id=\"rg_embed_analytics_90000099\"></iframe>"
        ),
        body
    );
    let json = serde_json::to_string(&html).unwrap();
    let mut literal = String::new();
    let mut chars = json.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' | '"' | '\'' | '`' | '$' => {
                literal.push('\\');
                literal.push(c);
            }
            '<' if chars.peek() == Some(&'/') => literal.push_str("<\\"),
            c => literal.push(c),
        }
    }
    format!(
        concat!(
            "if (document.getElementById('rg_embed_link_90000099')) {{\n",
            "  document.write('<link rel=\"stylesheet\" media=\"screen\" href=\"https://assets.genius.com/stylesheets/compiled/embedded_song-0.css\" />')\n",
            "  document.write(JSON.parse('{}'))\n",
            "  document.write('<script src=\"https://assets.genius.com/javascripts/compiled/embedded_song-0.js\" async=\"async\" crossorigin=\"true\"></script>')\n",
            "}}\n"
        ),
        literal
    )
}

/// The lines of an embed whose single `<p>` holds `inner`.
fn parse_p(inner: &str) -> Option<Vec<String>> {
    parse_body(&format!("\n  <p>{inner}</p>\n\n\n"))
}

fn parse_body(body: &str) -> Option<Vec<String>> {
    parse_embed(&embed_js(body)).expect("a readable embed")
}

fn lines(expected: &[&str]) -> Option<Vec<String>> {
    Some(expected.iter().map(|line| line.to_string()).collect())
}

/// A referent link (an explained fragment) as Genius writes it.
fn anchor(id: u32) -> String {
    format!(
        r#"<a href="https://genius.com/{id}/Nora-vale-fixture/Invented-referent-{id}" data-id="{id}" class="has_comments" data-editorial-state="accepted" data-classification="accepted" target="_blank" onclick="return false" data-genius-referent-id="{id}">"#
    )
}

#[test]
fn unescapes_javascript_strings() {
    assert_eq!(unescape_js(r#"a\'b\"c\\d\/e\$f\`g"#), "a'b\"c\\d/e$f`g");
    assert_eq!(unescape_js(r"1\n2\t3\r"), "1\n2\t3\r");
    assert_eq!(unescape_js(r"\x41\x42 \xZZ"), "AB xZZ");
    assert_eq!(unescape_js(r"café é"), "café é");
    assert_eq!(unescape_js(r"🌙 \u{1F319}"), "🌙 🌙");
    assert_eq!(unescape_js(r"lone \ud83c!"), "lone \u{fffd}!");
    assert_eq!(unescape_js(r"short \u12"), "short u12");
    assert_eq!(unescape_js("\\é and raw é"), "é and raw é");
    assert_eq!(unescape_js("line \\\ncontinued"), "line continued");
    assert_eq!(unescape_js("ends with \\"), "ends with ");
    assert_eq!(unescape_js(""), "");
}

#[test]
fn reads_an_embed_byte_for_byte_as_genius_writes_it() {
    // Header "[Letra de "...]" (its quote left open, as Genius keeps
    // it), section headers, quotes, apostrophes, "&amp;", a slash
    assert_eq!(
        parse_embed(PT_BR_HEADER).unwrap(),
        lines(&[
            "Acordei com o mundo de ponta-cabeça",
            "E avisou que o dia 'tava lento",
            "(Ei, ei, ei—)",
            "",
            "Comprei um sonho em \"promoção\", sem troco",
            "Dobrei a lua em papel e guardei no bolso",
            "D'água, d'ouro, d’ali: tudo cabe no meu som",
            "Rock & Roll de esquina, dia/noite sem horário",
            "Ninguém sabe o que é ser de lá — nem eu",
            "",
            "Lua de papel, lua de papel",
            "Amassa mas não rasga",
            "",
            "Coração de lata, açúcar no café",
            "Fé, fé, fé",
        ])
    );
    // The helper below writes embeds the same way
    let rebuilt = embed_js("\n  <p>Fé, \"fé\" &amp; \'fé\'<br>\nfim</p>\n\n\n");
    assert_eq!(
        parse_embed(&rebuilt).unwrap(),
        lines(&["Fé, \"fé\" & 'fé'", "fim"])
    );
}

#[test]
fn javascript_and_json_escapes_both_unescape() {
    assert_eq!(
        parse_embed(JS_ESCAPES).unwrap(),
        lines(&[
            "Ty Dolla $ign said it's 5$ a ticket",
            "Back`tick and \"double\" and 'single' quotes",
            "A backslash \\ in the middle",
            "Close tag lookalike: </p> stays text",
            "Path-ish: a/b/c and https://example.invalid/x",
        ])
    );
    // JS-level and JSON-level \u escapes, surrogate pairs, \x, \/
    assert_eq!(
        parse_embed(JS_UNICODE_ESCAPES).unwrap(),
        lines(&["Café café 🌙", "🌙 AB slash/ok", "end \"q\""])
    );
}

#[test]
fn explained_fragments_keep_their_text_wherever_they_start_and_end() {
    let html = format!(
        concat!(
            "{a1}[Intro]<br>\nPaper boats on a copper tide<br>\nNobody counts them as they leave<br>\n",
            "Hold up (Hold up, hold up)</a><br>\n<br>\n",
            "{a2}[Verse 1]<br>\nI painted every window shut<br>\nKept the light inside a jar</a><br>\n",
            "{a3}Gave the neighbours half my name</a><br>\nLeft the other half to the stars<br>\n",
            "and a {a4}quiet harbor, glass and rain</a>, just like that<br>\n",
            "{a5}Tide goes out, the ledger stays<br>\nMorning smells like diesel and salt</a>, oh<br>\n",
            "{a6}\u{200b}</a><br>\n<br>\n",
            "[Chorus]<br>\n{a7}Harbor, harbor, hold me slow<br>\nHarbor, harbor, let me go</a><br>\n<br>\n",
            "{a8}[Outro]<br>\nLights out on the water</a>"
        ),
        a1 = anchor(11000001),
        a2 = anchor(11000002),
        a3 = anchor(11000003),
        a4 = anchor(11000004),
        a5 = anchor(11000005),
        a6 = anchor(11000006),
        a7 = anchor(11000007),
        a8 = anchor(11000008),
    );
    assert_eq!(
        parse_p(&html),
        lines(&[
            "Paper boats on a copper tide",
            "Nobody counts them as they leave",
            "Hold up (Hold up, hold up)",
            "",
            "I painted every window shut",
            "Kept the light inside a jar",
            "Gave the neighbours half my name",
            "Left the other half to the stars",
            "and a quiet harbor, glass and rain, just like that",
            "Tide goes out, the ledger stays",
            "Morning smells like diesel and salt, oh",
            "",
            "Harbor, harbor, hold me slow",
            "Harbor, harbor, let me go",
            "",
            "Lights out on the water",
        ])
    );
}

#[test]
fn italics_and_bold_go_even_across_lines_and_inside_headers() {
    let html = format!(
        concat!(
            "[Verse 1: Nora Vale <i>&amp;</i> <b>Kit</b>]<br>\n",
            "I said (<i>wait</i>) and you said (<i>no</i>)<br>\n",
            "Everything <b>loud</b> sounds like home<br>\n",
            "<i>Whispered across two lines<br>\nand finished in italics</i><br>\n",
            "Plain again, then <b>bold that wraps<br>\nonto the next line</b> and out<br>\n<br>\n",
            "[Chorus: <i>Both</i>]<br>\n",
            "{a1}Say it <i>slow</i>, say it <b>twice</b></a><br>\n",
            "{a2}(<i>Say it</i>)</a>, say it again"
        ),
        a1 = anchor(11100001),
        a2 = anchor(11100002),
    );
    assert_eq!(
        parse_p(&html),
        lines(&[
            "I said (wait) and you said (no)",
            "Everything loud sounds like home",
            "Whispered across two lines",
            "and finished in italics",
            "Plain again, then bold that wraps",
            "onto the next line and out",
            "",
            "Say it slow, say it twice",
            "(Say it), say it again",
        ])
    );
}

#[test]
fn entities_are_decoded_once_after_the_tags_go() {
    let html = concat!(
        "Salt &amp; vinegar, rock &amp; roll<br>\n",
        "I &lt;3 you, 2 &gt; 1<br>\n",
        "She said &quot;go&quot; and I said &#39;no&#39;<br>\n",
        "Don&#x27;t wait, don&#8217;t stay<br>\n",
        "Non&nbsp;breaking&nbsp;space<br>\n",
        "&lt;b&gt;not bold&lt;/b&gt; is just text<br>\n",
        "AT&T, R&D and an unknown &foo; entity<br>\n",
        "&amp;lt; stays one level only<br>\n",
        "A raw I <3 you stays too"
    );
    assert_eq!(
        parse_p(html),
        lines(&[
            "Salt & vinegar, rock & roll",
            "I <3 you, 2 > 1",
            "She said \"go\" and I said 'no'",
            "Don't wait, don’t stay",
            "Non breaking space",
            "<b>not bold</b> is just text",
            "AT&T, R&D and an unknown &foo; entity",
            "&lt; stays one level only",
            "A raw I <3 you stays too",
        ])
    );
}

#[test]
fn section_headers_become_one_stanza_break() {
    let html = concat!(
        "[Intro]<br>\nfirst line right after a header<br>\nsecond line<br>\n",
        "[Verse 2]<br>\nheader glued to the previous line, no blank before it<br>\n<br>\n<br>\n",
        "[Pre-Chorus]<br>\n[Chorus]<br>\ntwo headers in a row and extra blanks<br>\n<br>\n",
        "[Wstęp: Nora, Kit, Razem/Chór]:<br>\n",
        "And if the lights go out before the bridge— [Gunshots]<br>\n",
        "Say it twice [x2]<br>\n(Verse 1)<br>\n[Verse 3: Nora [live]]<br>\n[]<br>\n",
        "  indented line with spaces  <br>\nends here<br>\n<br>\n[Refren: Razem/Chór]: <br>\n",
        "after a header with a colon<br>\n[Outro]."
    );
    assert_eq!(
        parse_p(html),
        lines(&[
            "first line right after a header",
            "second line",
            "",
            "header glued to the previous line, no blank before it",
            "",
            "two headers in a row and extra blanks",
            "",
            "And if the lights go out before the bridge— [Gunshots]",
            "Say it twice [x2]",
            "(Verse 1)",
            "",
            "indented line with spaces",
            "ends here",
            "",
            "after a header with a colon",
        ])
    );
}

#[test]
fn other_scripts_and_hidden_zero_width_spaces() {
    // Japanese, the localised header behind a zero-width space
    assert_eq!(
        parse_p(concat!(
            "\u{200b}[Nora Vale「月の紙」歌詞]<br>\n<br>\n[Verse 1]<br>\n",
            "朝の窓に月が残ってた<br>\n紙みたいに薄い光で<br>\nポケットに入れて歩いた<br>\n<br>\n",
            "[Chorus]<br>\nねえ、まだ眠れないの？<br>\nひらひら、ひらひら"
        )),
        lines(&[
            "朝の窓に月が残ってた",
            "紙みたいに薄い光で",
            "ポケットに入れて歩いた",
            "",
            "ねえ、まだ眠れないの？",
            "ひらひら、ひらひら",
        ])
    );
    assert_eq!(
        parse_p(concat!(
            "[노라 베일 \"종이 달\" 가사]<br>\n<br>\n[Verse 1: 노라 <i>&amp;</i> 케이]<br>\n",
            "밤하늘에 종이 달을 접어<br>\n주머니에 넣고 걸어가<br>\n(<i>오, 오, 오</i>)<br>\n<br>\n",
            "[Chorus]<br>\n아무도 몰라 내 이름"
        )),
        lines(&[
            "밤하늘에 종이 달을 접어",
            "주머니에 넣고 걸어가",
            "(오, 오, 오)",
            "",
            "아무도 몰라 내 이름",
        ])
    );
    assert_eq!(
        parse_p(concat!(
            "[المقطع الأول]<br>\nالقمر فوق البحر<br>\nوالليل هادئ<br>\n<br>\n",
            "[اللازمة]<br>\nأغني لك وحدي"
        )),
        lines(&["القمر فوق البحر", "والليل هادئ", "", "أغني لك وحدي"])
    );
}

#[test]
fn other_markup_still_reads() {
    assert_eq!(
        parse_body(concat!(
            "\n  <p>First paragraph, line one<br/>\nline two<br />\nline three<BR>\nline four</p>\n\n",
            "  <p>Second paragraph <span class=\"x\">with a span</span>, <em>em</em> and ",
            "<strong>strong</strong>.<br>\nraw newline inside\na line, joins with a space</p>\n\n\n"
        )),
        lines(&[
            "First paragraph, line one",
            "line two",
            "line three",
            "line four",
            "",
            "Second paragraph with a span, em and strong.",
            "raw newline inside a line, joins with a space",
        ])
    );
    // A translation page reads like any other (the search never picks it)
    assert_eq!(
        parse_p(concat!(
            "[Tradução de \"Lua de Papel\", de Nora Vale]<br>\n<br>\n[Introdução]<br>\n",
            "Eu acordei com o mundo de cabeça para baixo<br>\n<br>\n[Verso 1]<br>\n",
            "Comprei um sonho em promoção"
        )),
        lines(&[
            "Eu acordei com o mundo de cabeça para baixo",
            "",
            "Comprei um sonho em promoção",
        ])
    );
}

#[test]
fn songs_without_lyrics_give_none() {
    // Instrumental, also inside an explained fragment
    assert_eq!(parse_p("[Instrumental]"), None);
    assert_eq!(
        parse_p(&format!("{}[Instrumental]</a>", anchor(11200001))),
        None
    );
    // Unreleased: an empty body
    assert_eq!(parse_body("\n  \n"), None);
    assert_eq!(
        parse_p("Lyrics for this song have yet to be released"),
        None
    );
    assert_eq!(
        parse_p("Lyrics for this song have yet to be transcribed<br>\nPlease check back once the song has been released."),
        None
    );
    // A song that sings the word keeps its lyrics
    assert_eq!(
        parse_p("This part is instrumental<br>\nthen I come back in<br>\nand sing again"),
        lines(&[
            "This part is instrumental",
            "then I come back in",
            "and sing again"
        ])
    );
}

#[test]
fn an_embed_unlike_genius_is_an_error_not_an_empty_song() {
    assert!(parse_embed("").is_err());
    assert!(parse_embed("<html>Sorry</html>").is_err());
    // The literal never closes
    assert!(parse_embed("document.write(JSON.parse('\"abc").is_err());
    // Not JSON inside
    assert!(parse_embed("document.write(JSON.parse('<p>raw</p>'))").is_err());
    // No lyrics body
    assert!(
        parse_embed(r#"document.write(JSON.parse('\"<div class=\\\"other\\\">x<\/div>\"'))"#)
            .is_err()
    );
}

#[test]
fn long_explained_lyrics_come_whole() {
    let mut html = String::new();
    let mut expected: Vec<String> = Vec::new();
    for section in 1..=30u32 {
        if section > 1 {
            html.push_str("<br>\n<br>\n");
            expected.push(String::new());
        }
        html.push_str(&format!("[Verse {section}: Nora Vale]<br>\n"));
        for line in 1..=8u32 {
            if line > 1 {
                html.push_str("<br>\n");
            }
            let text =
                format!("Section {section}, line {line}: the copper harbor paints the orchard");
            // Every other line explained
            if line % 2 == 1 {
                html.push_str(&format!(
                    "{}{text}</a>",
                    anchor(12_000_000 + section * 100 + line)
                ));
            } else {
                html.push_str(&text);
            }
            expected.push(text);
        }
    }
    let js = embed_js(&format!("\n  <p>{html}</p>\n\n\n"));
    assert!(js.len() > 30_000, "{} bytes", js.len());
    assert_eq!(parse_embed(&js).unwrap(), Some(expected));
}

/// The same eight songs a search for "Nora Vale Lua de Papel" could
/// give: translations and other versions before the song itself.
fn hits_json(with_instrumental: bool) -> Vec<Value> {
    let hit = |id: u64,
               title: &str,
               artist_names: &str,
               primary: &str,
               state: &str,
               instrumental: bool| {
        let mut result = serde_json::json!({
            "_type": "song",
            "id": id,
            "title": title,
            "artist_names": artist_names,
            "primary_artist": { "name": primary },
            "lyrics_state": state,
            "url": format!("https://genius.com/fixture-{id}-lyrics"),
        });
        if with_instrumental {
            result["instrumental"] = Value::Bool(instrumental);
        }
        serde_json::json!({ "index": "song", "type": "song", "result": result })
    };
    vec![
        hit(
            90000101,
            "Nora Vale - Lua de Papel (Tradução em Português)",
            "Genius Brasil Traduções",
            "Genius Brasil Traduções",
            "complete",
            false,
        ),
        hit(
            90000102,
            "Lua de Papel",
            "Beto Cardoso",
            "Beto Cardoso",
            "complete",
            false,
        ),
        hit(
            90000001,
            "Lua de Papel",
            "Nora Vale (Ft. Dani Rocha)",
            "Nora Vale",
            "complete",
            false,
        ),
        hit(
            90000103,
            "Lua de Papel (Demo)",
            "Nora Vale",
            "Nora Vale",
            "unreleased",
            false,
        ),
        hit(
            90000104,
            "Lua de Papel (Instrumental)",
            "Nora Vale",
            "Nora Vale",
            "complete",
            true,
        ),
        hit(
            90000105,
            "Nora Vale - Lua de Papel (English Translation)",
            "Genius English Translations",
            "Genius English Translations",
            "complete",
            false,
        ),
        hit(
            90000106,
            "CÉU DE GIZ",
            "Lua Nova MC's",
            "Lua Nova MC\u{2019}s",
            "complete",
            false,
        ),
        hit(
            90000107,
            "Hustlin\u{2019} Hard",
            "Os Vale & Cia",
            "Os Vale",
            "complete",
            false,
        ),
    ]
}

fn public_search(hits: Vec<Value>) -> Value {
    serde_json::json!({
        "meta": { "status": 200 },
        "response": {
            "sections": [
                // A top hit section repeats a song and holds an artist
                { "type": "top_hit", "hits": [
                    { "index": "artist", "type": "artist", "result": { "id": 7, "name": "Nora Vale" } },
                    hits[2].clone()
                ] },
                { "type": "song", "hits": hits }
            ],
            "next_page": 2
        }
    })
}

fn official_search(hits: Vec<Value>) -> Value {
    serde_json::json!({ "meta": { "status": 200 }, "response": { "hits": hits } })
}

fn pick(hits: &[GeniusSongHit], artist: &str, title: &str) -> Option<u64> {
    pick_lyrics_hit(hits, artist, title).map(|hit| hit.id)
}

#[test]
fn both_search_answers_read_the_same() {
    let public = search_hits(&public_search(hits_json(true)));
    let official = search_hits(&official_search(hits_json(false)));
    let ids = |hits: &[GeniusSongHit]| hits.iter().map(|h| h.id).collect::<Vec<_>>();
    // The top hit comes first, once; the artist is no song
    assert_eq!(
        ids(&public),
        [90000001, 90000101, 90000102, 90000103, 90000104, 90000105, 90000106, 90000107]
    );
    assert_eq!(
        ids(&official),
        [90000101, 90000102, 90000001, 90000103, 90000104, 90000105, 90000106, 90000107]
    );
    let song = &official[2];
    assert_eq!(song.title, "Lua de Papel");
    assert_eq!(song.primary_artist, "Nora Vale");
    assert_eq!(song.artist_names, "Nora Vale (Ft. Dani Rocha)");
    assert_eq!(song.url, "https://genius.com/fixture-90000001-lyrics");
    assert_eq!(song.lyrics_state, "complete");
    assert!(public.iter().any(|h| h.instrumental));
    assert!(!official.iter().any(|h| h.instrumental), "absent means no");

    let empty_public = r#"{"meta":{"status":200},"response":{"sections":[{"type":"song","hits":[]}],"next_page":null}}"#;
    let empty_official = r#"{"meta":{"status":200},"response":{"hits":[]}}"#;
    let refused = r#"{"meta":{"status":401,"message":"This call requires an access_token. Please see: https://genius.com/developers"}}"#;
    for body in [empty_public, empty_official, refused] {
        assert!(search_hits(&serde_json::from_str(body).unwrap()).is_empty());
    }
}

#[test]
fn the_song_itself_is_picked_from_either_search() {
    for hits in [
        search_hits(&public_search(hits_json(true))),
        search_hits(&official_search(hits_json(false))),
    ] {
        assert_eq!(pick(&hits, "Nora Vale", "Lua de Papel"), Some(90000001));
        assert_eq!(
            pick(
                &hits,
                "Nora Vale, Dani Rocha",
                "Lua de Papel (feat. Dani Rocha)"
            ),
            Some(90000001)
        );
        assert_eq!(pick(&hits, "Beto Cardoso", "Lua de Papel"), Some(90000102));
        // Case, accents and a curly apostrophe in Genius's names
        assert_eq!(pick(&hits, "Lua Nova MC's", "Ceu de Giz"), Some(90000106));
        assert_eq!(pick(&hits, "Os Vale", "Hustlin' Hard"), Some(90000107));
        // Same title, someone else
        assert_eq!(pick(&hits, "Maria Souza", "Lua de Papel"), None);
        // Without the song, nothing else of the title is taken:
        // translations, the unreleased demo, the instrumental
        let others: Vec<GeniusSongHit> =
            hits.iter().filter(|h| h.id != 90000001).cloned().collect();
        assert_eq!(pick(&others, "Nora Vale", "Lua de Papel"), None);
    }
}

fn song(id: u64, title: &str, primary: &str, artist_names: &str) -> GeniusSongHit {
    GeniusSongHit {
        id,
        title: title.into(),
        primary_artist: primary.into(),
        artist_names: artist_names.into(),
        url: format!("https://genius.com/fixture-{id}-lyrics"),
        lyrics_state: "complete".into(),
        instrumental: false,
    }
}

#[test]
fn names_match_across_case_accents_and_apostrophes() {
    let hits = [song(14154298, "ALGO LÚCIDO", "FROID", "FROID")];
    assert_eq!(pick(&hits, "Froid", "Algo Lucido"), Some(14154298));
    assert_eq!(
        pick(&hits, "Froid", "Algo Lúcido - Ao Vivo"),
        Some(14154298)
    );
    assert_eq!(
        match_key("Straße Ñandú Łódź Æon"),
        "strasse nandu lodz aeon"
    );
    assert_eq!(match_key("MC’s  &  MCʼs"), "mcs and mcs");
    // "&" and "and" are one word
    let hits = [song(21, "Me and U", "Nora Vale", "Nora Vale")];
    assert_eq!(pick(&hits, "Nora Vale", "Me & U"), Some(21));
    // Accents as marks of their own
    assert_eq!(match_key("Lu\u{0301}cido"), "lucido");
}

#[test]
fn the_names_a_song_goes_by_match() {
    // Japanese and Korean pages carry both names
    let hits = [song(
        5638888,
        "夜に駆ける (Yoru ni Kakeru)",
        "YOASOBI",
        "YOASOBI",
    )];
    assert_eq!(pick(&hits, "YOASOBI", "夜に駆ける"), Some(5638888));
    assert_eq!(pick(&hits, "YOASOBI", "Yoru ni Kakeru"), Some(5638888));
    let hits = [song(
        764417,
        "봄날 (Spring Day)",
        "BTS (방탄소년단)",
        "BTS (방탄소년단)",
    )];
    assert_eq!(pick(&hits, "BTS", "Spring Day"), Some(764417));
    // A soundtrack edition on Spotify, the song on Genius
    let hits = [song(1, "Let It Go", "Idina Menzel", "Idina Menzel")];
    assert_eq!(
        pick(
            &hits,
            "Idina Menzel",
            "Let It Go - From \"Frozen\"/Soundtrack Version"
        ),
        Some(1)
    );
    // A feature written "part." (Portuguese) is not a part number
    let hits = [song(2, "Coração", "Nora Vale", "Nora Vale")];
    assert_eq!(pick(&hits, "Nora Vale", "Coração (part. Froid)"), Some(2));
    // A longer Genius title around the track's
    let hits = [song(
        3,
        "Shakira: Bzrp Music Sessions, Vol. 53",
        "Bizarrap",
        "Bizarrap & Shakira",
    )];
    assert_eq!(
        pick(&hits, "Bizarrap, Shakira", "Bzrp Music Sessions, Vol. 53"),
        Some(3)
    );
    assert_eq!(
        pick(&hits, "Shakira, Bizarrap", "Bzrp Music Sessions, Vol. 53"),
        Some(3)
    );
    // Someone else's song with the track's main artist on it
    let hits = [song(
        4,
        "Lua de Papel",
        "Nora Vale",
        "Nora Vale (Ft. Dani Rocha)",
    )];
    assert_eq!(pick(&hits, "Dani Rocha", "Lua de Papel"), Some(4));
}

#[test]
fn other_songs_versions_and_parts_are_never_taken() {
    // A remix with the artist on it
    let remix = song(
        9,
        "HUMBLE. (SKRILLEX REMIX)",
        "Skrillex",
        "Skrillex & Kendrick Lamar",
    );
    assert_eq!(pick(&[remix.clone()], "Kendrick Lamar", "HUMBLE."), None);
    let original = song(3039923, "HUMBLE.", "Kendrick Lamar", "Kendrick Lamar");
    assert_eq!(
        pick(&[remix, original], "Kendrick Lamar", "HUMBLE."),
        Some(3039923)
    );
    // A live page only for a live track
    let live = [song(
        10,
        "Numb (Live in Texas)",
        "Linkin Park",
        "Linkin Park",
    )];
    assert_eq!(pick(&live, "Linkin Park", "Numb"), None);
    assert_eq!(pick(&live, "Linkin Park", "Numb (Live in Texas)"), Some(10));
    // A title inside a much longer one is another song
    let love_me_do = [song(11, "Love Me Do", "The Beatles", "The Beatles")];
    assert_eq!(pick(&love_me_do, "The Beatles", "Love"), None);
    let love = [song(12, "Love", "The Beatles", "The Beatles")];
    assert_eq!(pick(&love, "The Beatles", "Love Me Do"), None);
    // Another part, either way
    let part_two = [song(13, "Algo Lúcido, Pt. 2", "Froid", "Froid")];
    assert_eq!(pick(&part_two, "Froid", "Algo Lúcido"), None);
    let part_one = [song(14, "Algo Lúcido", "Froid", "Froid")];
    assert_eq!(pick(&part_one, "Froid", "Algo Lúcido (Parte 2)"), None);
    let session = [song(
        15,
        "Bzrp Music Sessions, Vol. 52",
        "Bizarrap",
        "Bizarrap",
    )];
    assert_eq!(
        pick(&session, "Bizarrap", "Bzrp Music Sessions, Vol. 53"),
        None
    );
    // Translation pages, whatever they are called
    let romanized = [
        song(
            16,
            "YOASOBI - 夜に駆ける (Yoru ni Kakeru) (Romanized)",
            "Genius Romanizations",
            "Genius Romanizations",
        ),
        song(17, "夜に駆ける (English Translation)", "YOASOBI", "YOASOBI"),
        song(
            18,
            "夜に駆ける",
            "Polskie tłumaczenia Genius",
            "Polskie tłumaczenia Genius",
        ),
    ];
    assert_eq!(pick(&romanized, "YOASOBI", "夜に駆ける"), None);
    // Unreleased or instrumental, even with the right name
    let mut unreleased = song(19, "Algo Lúcido", "Froid", "Froid");
    unreleased.lyrics_state = "unreleased".into();
    let mut instrumental = song(20, "Algo Lúcido", "Froid", "Froid");
    instrumental.instrumental = true;
    assert_eq!(
        pick(&[unreleased, instrumental], "Froid", "Algo Lúcido"),
        None
    );
    // Nothing to compare
    assert_eq!(pick(&part_one, "", "Algo Lúcido"), None);
    assert_eq!(pick(&part_one, "Froid", ""), None);
}

#[test]
fn without_the_song_its_siblings_are_never_taken() {
    // Another number is another song, either way
    for (track, hit) in [
        ("Alexa", "ALEXA 2"),
        ("Song 2", "Song"),
        ("Tema", "Tema II"),
        ("Fique Rico ou Moralismo", "Fique Rico ou Moralismo 2"),
        ("Lua de Papel", "Lua de Papel (2)"),
    ] {
        let hits = [song(30, hit, "Nora Vale", "Nora Vale")];
        assert_eq!(pick(&hits, "Nora Vale", track), None, "{track} took {hit}");
    }
    let hits = [song(31, "Fique Rico ou Moralismo 2", "Froid", "Froid")];
    assert_eq!(
        pick(&hits, "Froid, Major RD", "Fique Rico ou Moralismo 2"),
        Some(31)
    );
    // Other takes and pieces of an album, either way
    for hit in [
        "Solo (Reprise)",
        "Solo (Skit)",
        "Solo (Acoustic)",
        "Solo (Extended)",
        "Solo (Radio Edit)",
        "Solo (Club Mix)",
        "Solo (Edit)",
        "Solo (Version)",
        "Solo (Piano)",
        "Solo (VIP)",
        "Solo (Interlúdio)",
    ] {
        let hits = [song(32, hit, "Nora Vale", "Nora Vale")];
        assert_eq!(pick(&hits, "Nora Vale", "Solo"), None, "Solo took {hit}");
    }
    let hits = [song(33, "Solo", "Nora Vale", "Nora Vale")];
    assert_eq!(pick(&hits, "Nora Vale", "Solo (Reprise)"), None);
    assert_eq!(pick(&hits, "Nora Vale", "Solo - Skit"), None);
    // One word is never found inside a longer title
    let hits = [song(34, "Solo Dolo", "Nora Vale", "Nora Vale")];
    assert_eq!(pick(&hits, "Nora Vale", "Solo"), None);
    // "Intro" and the like: too common to tell apart by name
    let hits = [
        song(35, "Humor Negro (Intro)", "Nora Vale", "Nora Vale"),
        song(36, "Intro", "Nora Vale", "Nora Vale"),
    ];
    assert_eq!(pick(&hits, "Nora Vale", "Intro"), None);
    assert_eq!(pick(&hits, "Nora Vale", "Outro 2"), None);
    assert_eq!(pick(&hits, "Nora Vale", "Humor Negro"), None);
    // Someone else's song the artist is credited on: only by the very
    // same title ("A Língua dos Anjos" is another song)
    let other = [song(
        37,
        "A Língua dos Anjos",
        "Orquestra Novo Traço",
        "Orquestra Novo Traço (Ft. Froid)",
    )];
    assert_eq!(pick(&other, "Froid", "Língua dos Anjos"), None);
    let credited = [song(
        38,
        "Dispiei",
        "Pirâmide Perdida",
        "Pirâmide Perdida (Ft. BK', Bril, Nectar Gang & Sain)",
    )];
    assert_eq!(pick(&credited, "Sain", "Dispiei"), Some(38));
}

#[test]
fn two_songs_matching_alike_are_none_and_the_same_version_wins() {
    // Two pages of one title by one artist can't be told apart
    let twins = [
        song(40, "Lua de Papel", "Nora Vale", "Nora Vale"),
        song(41, "Lua de Papel", "Nora Vale", "Nora Vale"),
    ];
    assert_eq!(pick(&twins, "Nora Vale", "Lua de Papel"), None);
    // A better match still wins over them
    let mut hits = twins.to_vec();
    hits.push(song(42, "Lua de Papel (Ao Vivo)", "Nora Vale", "Nora Vale"));
    assert_eq!(pick(&hits, "Nora Vale", "Lua de Papel (Ao Vivo)"), Some(42));
    // The live track takes the live page, whatever Genius's order; the
    // studio track never takes it
    let studio = song(43, "Colômbia", "Froid", "Froid");
    let live = song(44, "Colômbia (Ao Vivo)", "Froid", "Froid");
    for hits in [
        [studio.clone(), live.clone()],
        [live.clone(), studio.clone()],
    ] {
        assert_eq!(pick(&hits, "Froid", "Colômbia (Ao Vivo)"), Some(44));
        assert_eq!(pick(&hits, "Froid", "Colômbia - Ao Vivo"), Some(44));
        assert_eq!(pick(&hits, "Froid", "Colômbia"), Some(43));
    }
}

/// A Genius as `server`, for the look-up to ask.
fn endpoints(server: &crate::test_server::TestServer) -> GeniusEndpoints {
    GeniusEndpoints {
        public_search: format!("{}/api/search/song", server.base),
        api_search: format!("{}/official/search", server.base),
        songs: format!("{}/songs", server.base),
    }
}

const ONE_LINE_EMBED: &str = "\n  <p>[Intro]<br>\nPaper boats on a copper tide</p>\n\n\n";

async fn look_up(
    server: &crate::test_server::TestServer,
    token: &str,
) -> Result<Option<GeniusLyrics>, DomainError> {
    let client = crate::test_server::client();
    let at = endpoints(server);
    let found = fetch_genius_lyrics_from(&at, &client, "Nora Vale", "Lua de Papel", token).await;
    found
}

#[tokio::test]
async fn a_refused_token_searches_without_it() {
    use crate::test_server::TestServer;
    let public = public_search(hits_json(true)).to_string();
    let embed = embed_js(ONE_LINE_EMBED);
    let server = TestServer::start(&[
        ("/official/search", 401, r#"{"meta":{"status":401}}"#),
        ("/api/search/song", 200, public.as_str()),
        ("/songs/90000001/embed.js", 200, embed.as_str()),
    ]);
    let found = look_up(&server, "test-token").await.unwrap().unwrap();
    assert_eq!(found.song_id, 90000001);
    assert_eq!(found.lines, ["Paper boats on a copper tide"]);
    // The token went to the official API only
    let official = server.requests("/official/search");
    assert_eq!(official.len(), 1);
    assert!(official[0]
        .to_ascii_lowercase()
        .contains("authorization: bearer"));
    let searches = server.requests("/api/search/song");
    assert_eq!(searches.len(), 1);
    assert!(!searches[0].to_ascii_lowercase().contains("authorization"));
    assert!(searches[0].contains("q=Nora+Vale+Lua+de+Papel"));
}

#[tokio::test]
async fn genius_refusing_never_reads_as_an_expired_login() {
    use crate::test_server::TestServer;
    let server = TestServer::start(&[("/api/search/song", 403, "<html>blocked</html>")]);
    let error = look_up(&server, "").await.unwrap_err();
    assert!(
        matches!(error, DomainError::ClientError { status: 403, .. }),
        "{error:?}"
    );
    // Neither with a token both searches refuse
    let server = TestServer::start(&[
        ("/official/search", 401, "{}"),
        ("/api/search/song", 401, "{}"),
    ]);
    let error = look_up(&server, "test-token").await.unwrap_err();
    assert!(!matches!(error, DomainError::AuthExpired), "{error:?}");
}

#[tokio::test]
async fn the_second_query_finds_what_the_first_missed() {
    use crate::test_server::TestServer;
    // Only translation pages come back for "artist title"
    let only_translations = serde_json::json!({ "response": { "sections": [
        { "type": "song", "hits": [hits_json(true)[0].clone(), hits_json(true)[5].clone()] }
    ] } })
    .to_string();
    let public = public_search(hits_json(true)).to_string();
    let embed = embed_js(ONE_LINE_EMBED);
    let server = TestServer::start(&[
        ("q=Nora+Vale+Lua", 200, only_translations.as_str()),
        ("q=Lua+de+Papel+Nora", 200, public.as_str()),
        ("/songs/90000001/embed.js", 200, embed.as_str()),
    ]);
    let found = look_up(&server, "").await.unwrap().unwrap();
    assert_eq!(found.song_id, 90000001);
    assert_eq!(server.requests("/api/search/song").len(), 2);
}

#[tokio::test]
async fn the_embed_answer_decides_none_or_an_error() {
    use crate::test_server::TestServer;
    let public = public_search(hits_json(true)).to_string();
    let answer = |status| {
        let server = TestServer::start(&[
            ("/api/search/song", 200, public.as_str()),
            ("/songs/90000001/embed.js", status, "{}"),
        ]);
        async move { look_up(&server, "").await }
    };
    // No such song: no lyrics, not an error
    assert_eq!(answer(404).await.unwrap(), None);
    assert!(matches!(
        answer(429).await,
        Err(DomainError::RateLimited { .. })
    ));
    assert!(matches!(
        answer(503).await,
        Err(DomainError::ServerError { status: 503, .. })
    ));
    // Not an embed this knows
    assert!(matches!(answer(200).await, Err(DomainError::Parse(_))));
}

#[tokio::test]
async fn an_intro_is_never_searched_for() {
    use crate::test_server::TestServer;
    let server = TestServer::start(&[]);
    let client = crate::test_server::client();
    let found = fetch_genius_lyrics_from(&endpoints(&server), &client, "Nora Vale", "Intro", "")
        .await
        .unwrap();
    assert_eq!(found, None);
    assert!(server.requests("/").is_empty());
}

#[test]
fn song_pages_stay_on_genius() {
    let mut hit = song(14154298, "Algo Lúcido", "Froid", "Froid");
    hit.url = "https://genius.com/Froid-algo-lucido-lyrics".into();
    assert_eq!(
        song_page(&hit),
        "https://genius.com/Froid-algo-lucido-lyrics"
    );
    hit.url = "javascript:alert(1)".into();
    assert_eq!(song_page(&hit), "https://genius.com/songs/14154298");
    hit.url = String::new();
    assert_eq!(song_page(&hit), "https://genius.com/songs/14154298");
}

/// The real Genius, without a token:
/// `cargo test -p canto-api --lib -- --ignored live_genius_lyrics`.
/// Prints nothing: lyrics are copyrighted, and none are kept here.
#[tokio::test]
#[ignore]
async fn live_genius_lyrics() {
    let client = reqwest::Client::new();
    let found = fetch_genius_lyrics(&client, "Froid", "Algo Lúcido", "")
        .await
        .expect("Genius answers")
        .expect("Genius has the lyrics");
    assert_eq!(found.song_id, 14154298);
    assert_eq!(found.url, "https://genius.com/Froid-algo-lucido-lyrics");
    assert!(found.lines.len() > 20, "{} lines", found.lines.len());
    // The title is sung in it
    assert!(found
        .lines
        .iter()
        .any(|line| match_key(line).contains("algo lucido")));
    assert!(!found
        .lines
        .iter()
        .any(|line| line.starts_with('[') && line.ends_with(']')));
}
