use texplitter_rs::parserrules::load_regex_patterns;
use texplitter_rs::tokenizer::Tokenizer;
use log::LevelFilter;
use simplelog::{Config, SimpleLogger};

#[test]
fn test_regex_patterns() {
    let _ = SimpleLogger::init(LevelFilter::Debug, Config::default());
    let patterns = load_regex_patterns("../src/texplitter/config/rules/default.json").unwrap();
    let mut tokenizer = Tokenizer::new(patterns.clone()).expect("Failed to initialize tokenizer");

    for pattern in &patterns {
        let Some(sample) = pattern.sample.as_deref() else {
            continue;
        };
        println!("Testing pattern {}/{} sample={:?}", pattern.class, pattern.r#type, sample);
        match tokenizer.tokenize(sample) {
            Ok(tokens) => {
                let token = &tokens[0];
                assert_eq!(pattern.r#type, token["token_type"].as_str().unwrap());
                assert_eq!(pattern.class, token["token_class"].as_str().unwrap());
            },
            Err(e) => println!("Failed to tokenize sample: {}. Error: {:?}", sample, e),
        }
        println!("{}", ".".repeat(20));
    }
}

fn create_tokenizer() -> Tokenizer {
    // Load regex patterns using parserrules.rs
    let patterns = load_regex_patterns("../src/texplitter/config/rules/default.json").expect("Failed to load regex patterns");

    // Create a new Tokenizer with the regex patterns
    Tokenizer::new(patterns).expect("Failed to create Tokenizer")
}

#[test]
fn test_nesting_errors() {
    let _ = SimpleLogger::init(LevelFilter::Debug, Config::default());
    let mut tokenizer = create_tokenizer();
    let input_text = r#"
        <div>
            <p>This is a paragraph with a <strong>bold text</p></strong>
        </div>
    "#;

    match tokenizer.tokenize(input_text) {
        Ok(tokens) => {
            println!("Tokens: {:?}", tokens);
            let nesting_errors = tokenizer.get_nesting_errors();
            println!("Nesting errors: {:?}", nesting_errors);
            assert!(!nesting_errors.is_empty(), "Nesting errors should not be empty");
        }
        Err(e) => panic!("Tokenization failed: {:?}", e),
    }
}

#[test]
fn test_unmatched_tags() {
    let _ = SimpleLogger::init(LevelFilter::Debug, Config::default());
    let mut tokenizer = create_tokenizer();
    let input_text = r#"
        <div>
            <p>This is a paragraph with a <strong>bold text</strong>
        </div>
    "#;

    match tokenizer.tokenize(input_text) {
        Ok(tokens) => {
            println!("Tokens: {:?}", tokens);
            let unmatched_tags = tokenizer.get_unmatched_tags();
            println!("Unmatched tags: {:?}", unmatched_tags);
            assert!(!unmatched_tags.is_empty(), "Unmatched tags should not be empty");
        }
        Err(e) => panic!("Tokenization failed: {:?}", e),
    }
}

#[test]
fn test_tokenizer_with_complex_html() {
    let _ = SimpleLogger::init(LevelFilter::Debug, Config::default());
    let segment = r#"
    <p>See if this is a sentence.</p>
    <html lang="en">
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <title>Sample HTML</title>
        <link rel="stylesheet" type="text/css" href="_static/sample.css" />
      </head>
      <body>
        <p>See if this is a sentence.</p>
        <div class="document">
          <div class="documentwrapper">
            <div class="bodywrapper">
              <div class="body" role="main">
                <section id="sample-section">
                  <h1>Sample Section.<a class="headerlink" href="sample-section" title="Link to this heading">¶</a></h1>
                  <div class="toctree-wrapper compound"></div>
                  <section id="sample-subsection">
                    <h2>Sample Subsection.<a class="headerlink" href="sample-subsection" title="Link to this heading">¶</a></h2>
                    <dl class="py class" id="sample-class">
                      <dt class="sig sig-object py" id="sample-id">
                        <span class="sig-name descname">Sample Class</span>
                        <a class="headerlink" href="sample-id" title="Link to this definition">¶</a>
                      </dt>
                      <dd>
                        <p>A simple sample class for testing.</p>
                      </dd>
                    </dl>
                  </section>
                </section>
              </div>
            </div>
          </div>
        </div>
        <p>See if this is another sentence.</p>
        <div class="footer">
          &#169; 2024 Sample Footer
        </div>
      </body>
    </html>
    "#;

    let patterns = load_regex_patterns("../src/texplitter/config/rules/default.json").unwrap();
    let mut tokenizer = Tokenizer::new(patterns).expect("Failed to initialize tokenizer");
    let tokens = tokenizer.tokenize(segment).expect("Failed to tokenize segment");
    let unmatched_tags = tokenizer.get_unmatched_tags();
    let nesting_errors = tokenizer.get_nesting_errors();

    println!("{:?}", tokens);
    println!("{}", "*".repeat(40));
    println!("Unmatched Tags:");
    println!("{:?}", unmatched_tags);
    println!("{}", "*".repeat(40));
    println!("Nesting Errors:");
    println!("{:?}", nesting_errors);
    println!("{}", "*".repeat(40));

    assert!(unmatched_tags.is_empty());
    assert!(nesting_errors.is_empty());
}
