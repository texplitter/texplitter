import pytest
from texplitter.tokenizer import Tokenizer, UnknownTokenError
from texplitter.parserrules import regex_patterns
import json

@pytest.fixture
def tokenizer():
    return Tokenizer()

def test_tokenizer_initialization(tokenizer: Tokenizer):
    assert tokenizer is not None

@pytest.mark.parametrize("pattern", regex_patterns)
def test_regex_patterns(tokenizer: Tokenizer, pattern):
    sample = pattern['sample']
    if isinstance(sample, list):
        sample = sample[0]  # Use the first sample if there are multiple samples
    tokens = tokenizer.tokenize(sample)
    print(pattern)
    print('.'*20)
    print(tokens)
#    assert len(tokens) == 1
    token = tokens[0]
    assert pattern['type'] == token['token_type'] 
    assert pattern['class'] == token['token_class'] 
#    assert token['text'] == sample.strip()
#    assert token['label'] == pattern['type']

def test_unmatched_tags(tokenizer: Tokenizer):
    sample = '''
    <div>
        <p>This is a paragraph with an <strong>important</strong> part.
        <p>This paragraph is missing a closing tag.
    </div>
    <div>
        <span>Another span without closing.
    </div>
    '''
    tokenizer.tokenize(sample)
    unmatched_tags = tokenizer.get_unmatched_tags()
    assert len(unmatched_tags) > 0

def test_nesting_errors(tokenizer: Tokenizer):
    sample = '''
    <div>
        <p>This is a paragraph with a <strong>bold text</p></strong>
    </div>
    '''
    tokenizer.tokenize(sample)
    nesting_errors = tokenizer.get_nesting_errors()
    assert len(nesting_errors) > 0

def test_tokenizer_with_complex_html():
    segment = '''
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
                  <h1>Sample Section.<a class="headerlink" href="#sample-section" title="Link to this heading">¶</a></h1>
                  <div class="toctree-wrapper compound"></div>
                  <section id="sample-subsection">
                    <h2>Sample Subsection.<a class="headerlink" href="#sample-subsection" title="Link to this heading">¶</a></h2>
                    <dl class="py class" id="sample-class">
                      <dt class="sig sig-object py" id="sample-id">
                        <span class="sig-name descname">Sample Class</span>
                        <a class="headerlink" href="#sample-id" title="Link to this definition">¶</a>
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
    '''

    tokenizer = Tokenizer()
    tokens = tokenizer.tokenize(segment)
    unmatched_tags = tokenizer.get_unmatched_tags()
    nesting_errors = tokenizer.get_nesting_errors()

    print(tokens)
    print('*' * 40)
    print("Unmatched Tags:")
    print(json.dumps(unmatched_tags, sort_keys=False, indent=4))
    print('*' * 40)
    print("Nesting Errors:")
    print(json.dumps(nesting_errors, sort_keys=False, indent=4))
    print('*' * 40)

    assert len(unmatched_tags) == 0
    assert len(nesting_errors) == 0

if __name__ == '__main__':
    pytest.main()
