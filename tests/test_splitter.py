import pytest
import logging
from texplitter.splitter import TreeNode, SegmentTree, convert_tokens_to_string, replace_substring_from_end
from texplitter.tokenizer import Tokenizer
from texplitter._native import InvalidNodeDictError, InvalidTokenError

logging.basicConfig(level=logging.DEBUG, format='%(asctime)s - %(levelname)s - %(message)s')
def test_convert_tokens_to_string():
    tokens = [{'text': 'Hello'}, {'text': ' '}, {'text': 'World'}]
    result = convert_tokens_to_string(tokens)
    assert result == 'Hello World'

def test_replace_substring_from_end():
    text = 'hello world world'
    result = replace_substring_from_end(text, 'world', 'everyone', 1)
    assert result == 'hello world everyone'

def test_tree_node_creation():
    node = TreeNode('SINGLE', 'value', ['<tag>', '</tag>'])
    assert node.node_type == 'SINGLE'
    assert node.value == 'value'
    assert node.tag == ['<tag>', '</tag>']

def test_tree_node_build_full_value():
    child = TreeNode('TRANSLATABLE', 'child_value', '')
    node = TreeNode('SINGLE', 'value', ['<tag>', '</tag>'], children=[child])
    result = node.build_full_value()
    assert result == '<tag>child_value</tag>'

def test_get_attributes():
    # Pre-order DFS walk: parent attrs before children, left-to-right among
    # siblings. The Rust-backed `get_attributes()` flattens the whole
    # subtree into a single ordered list of (key, value) tuples.
    root = TreeNode('SINGLE', 'root', ['<root>', '</root>'], children=[
        TreeNode('SINGLE', 'child1', ['<c1>', '</c1>'],
                 attributes=[('attr1_key', 'attr1_value')]),
        TreeNode('SINGLE', 'child2', ['<c2>', '</c2>'],
                 attributes=[('attr2_key', 'attr2_value')],
                 children=[
                     TreeNode('SINGLE', 'grandchild1', ['<g1>', '</g1>'],
                              attributes=[('attr3_key', 'attr3_value')]),
                     TreeNode('SINGLE', 'grandchild2', ['<g2>', '</g2>'],
                              attributes=[('attr4_key', 'attr4_value')]),
                 ]),
    ])

    expected_attributes = [
        ('attr1_key', 'attr1_value'),
        ('attr2_key', 'attr2_value'),
        ('attr3_key', 'attr3_value'),
        ('attr4_key', 'attr4_value'),
    ]
    assert root.get_attributes() == expected_attributes

def test_tree_node_extract_segments():
    child = TreeNode('TRANSLATABLE', 'child_value', '')
    node = TreeNode('SINGLE', 'value', ['<tag>', '</tag>'], children=[child])
    result = node.extract_segments()
    # `extract_segments()` returns a single-element list of flat-merged
    # `{tag: text}` maps. The iterative walker in splitter.rs merges child
    # maps at FinalizeSingle time.
    expected = [{'TRANSLATABLE/value-0': 'child_value'}]
    assert result == expected

def test_generate_dot_representation():
    # Build the tree structure
    child = TreeNode('TRANSLATABLE', 'child_value', '')
    node = TreeNode('SINGLE', 'value', ['<tag>', '</tag>'], children=[child])

    # Generate DOT representation
    dot_representation = node.generate_dot_representation()
    print(dot_representation)
    # Expected output for the given tree structure
    expected = '''
digraph G {
    rankdir = TB
    fontname = "DejaVu"
    fontsize = 10

    node [
        fontname = "DejaVu"
        fontsize = 10
        shape = "record"
    ]

    edge [
        fontname = "DejaVu"
        fontsize = 10
    ]

    "node0" [label="{<f0> node_type= SINGLE|value| { <f1> }}", fontcolor=red];
    "node1" [label="{<f0> node_type= TRANSLATABLE|child_value}", fontcolor=blue, shape=Mrecord];
    "node0":f1 -> "node1":f0 ;
}
    '''

    assert dot_representation.strip() == expected.strip()

def test_generate_svg():
    # Build the tree structure
    child = TreeNode('TRANSLATABLE', 'child_value', '')
    node = TreeNode('SINGLE', 'value', ['<tag>', '</tag>'], children=[child])

    svg = node.generate_svg()
    print(svg)

    # The SVG output is produced by `xmlwriter` with integer coordinates.
    # This test asserts the properties that make the SVG well-formed and
    # semantically correct:
    #
    #   1. Parseable XML with `<svg>` as the document element carrying
    #      the SVG namespace.
    #   2. One `<g id="node0">` and one `<g id="node1">` present as
    #      siblings of a single connector `<line>`, all direct
    #      children of `<svg>`. (If the children were nested inside
    #      the parent `<g>` a transform on the parent would cascade to
    #      the child — the bug this test originally guarded against.)
    #   3. Every `<g>` contains a `<rect>` and the node's type /
    #      value text.
    import xml.etree.ElementTree as ET

    root = ET.fromstring(svg)
    # ElementTree moves the xmlns into the tag as `{ns}localname`.
    assert root.tag == '{http://www.w3.org/2000/svg}svg', \
        f'root is {root.tag!r}, expected svg element in svg namespace'

    ns = {'s': 'http://www.w3.org/2000/svg'}

    # Top-level <g> groups must be siblings of the connector <line>,
    # not nested inside the parent <g>. Nesting would make a transform
    # on the parent cascade to the child — the bug this test guards
    # against.
    top_level_groups = root.findall('s:g', ns)
    top_level_lines = root.findall('s:line', ns)

    group_ids = {g.attrib.get('id') for g in top_level_groups}
    assert 'node0' in group_ids, f'root <g id="node0"> missing; got {group_ids}'
    assert 'node1' in group_ids, f'child <g id="node1"> missing; got {group_ids}'
    assert len(top_level_lines) == 1, \
        f'expected exactly one connector <line>, got {len(top_level_lines)}'

    # Each <g> must contain a rect plus text elements carrying the
    # node type and value.
    for g in top_level_groups:
        rects = g.findall('s:rect', ns)
        texts = g.findall('s:text', ns)
        assert len(rects) == 1, f'<g id={g.attrib.get("id")}> missing rect'
        assert len(texts) >= 2, \
            f'<g id={g.attrib.get("id")}> needs type+value text nodes'

    node0 = next(g for g in top_level_groups if g.attrib.get('id') == 'node0')
    node0_texts = [(t.text or '').strip() for t in node0.findall('s:text', ns)]
    assert 'SINGLE' in node0_texts
    assert 'value' in node0_texts

    node1 = next(g for g in top_level_groups if g.attrib.get('id') == 'node1')
    node1_texts = [(t.text or '').strip() for t in node1.findall('s:text', ns)]
    assert 'TRANSLATABLE' in node1_texts
    assert 'child_value' in node1_texts

def test_segment_tree_creation():
    tokens = [{'token_class': 'OPEN_BRACKET', 'text': '<tag>', 'label': 'tag'}, {'token_class': 'TRANSLATABLE', 'text': 'text', 'label': 'text'}, {'token_class': 'CLOSE_BRACKET', 'text': '</tag>', 'label': 'tag'}]
    tree = SegmentTree(tokens)
    assert tree.tree.node_type == 'MULTIPLE'

def test_complex_html_segment():
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

    # Initialize the lexer and scan the segment
    tokenizer = Tokenizer()
    tokens = tokenizer.tokenize(segment)
    unmatched_tags = tokenizer.get_unmatched_tags()
    nesting_errors = tokenizer.get_nesting_errors()

    # Create a SegmentTree instance
    segment_tree = SegmentTree(tokens)
    attributes = segment_tree.tree.get_attributes()
    # Extract segments
    # Extract extended segments for machine translation
    extended_segments = segment_tree.tree.extract_extended_segments()

    # Generate post-editing segments and template
    __, template, post_editing_segments = segment_tree.tree.generate_placeholders_for_segments('')

    print(f"Generated template: {template}")
    print(f"Generated post-editing segments: {post_editing_segments}")


    expected_template = '''
<p>%%1%%</p>
<html lang="@html/lang~0@">
  <head>
    <meta charset="@meta/charset~0@" />
    <meta name="@meta/name~0@" content="@meta/content~0@" />
    <title>%%2%%</title>
    <link rel="@link/rel~0@" type="@link/type~0@" href="@link/href~0@" />
  </head>
  <body>
    <p>%%3%%</p>
    <div class="@div/class~0@">
      <div class="@div/class~1@">
        <div class="@div/class~2@">
          <div class="@div/class~3@" role="@div/role~0@">
            <section id="@section/id~0@">
              <h1>%%4%%</h1>
              <div class="@div/class~4@"></div>
              <section id="@section/id~1@">
                <h2>%%5%%</h2>
                <dl class="@dl/class~0@" id="@dl/id~0@">
                  <dt class="@dt/class~0@" id="@dt/id~0@">
                    <span class="@span/class~0@">%%6%%</span>
                    <a class="@a/class~2@" href="@a/href~2@" title="@a/title~2@">%%7%%</a>
                  </dt>
                  <dd>
                    <p>%%8%%</p>
                  </dd>
                </dl>
              </section>
            </section>
          </div>
        </div>
      </div>
    </div>
    <p>%%9%%</p>
    <div class="@div/class~5@">
      %%10%%
    </div>
  </body>
</html>
    '''

    expected_attributes = [('html/lang~0', 'en'), ('meta/charset~0', 'utf-8'), ('meta/name~0', 'viewport'), ('meta/content~0', 'width=device-width, initial-scale=1.0'), ('link/rel~0', 'stylesheet'), ('link/type~0', 'text/css'), ('link/href~0', '_static/sample.css'), ('div/class~0', 'document'), ('div/class~1', 'documentwrapper'), ('div/class~2', 'bodywrapper'), ('div/class~3', 'body'), ('div/role~0', 'main'), ('section/id~0', 'sample-section'), ('a/class~0', 'headerlink'), ('a/href~0', '#sample-section'), ('a/title~0', 'Link to this heading'), ('div/class~4', 'toctree-wrapper compound'), ('section/id~1', 'sample-subsection'), ('a/class~1', 'headerlink'), ('a/href~1', '#sample-subsection'), ('a/title~1', 'Link to this heading'), ('dl/class~0', 'py class'), ('dl/id~0', 'sample-class'), ('dt/class~0', 'sig sig-object py'), ('dt/id~0', 'sample-id'), ('span/class~0', 'sig-name descname'), ('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition'), ('div/class~5', 'footer')]


    expected_post_editing_segments = [
        (1, 'See if this is a sentence.'),
        (2, 'Sample HTML'),
        (3, 'See if this is a sentence.'),
        (4, 'Sample Section.<a class="@a/class~0@" href="@a/href~0@" title="@a/title~0@">¶</a>'),
        (5, 'Sample Subsection.<a class="@a/class~1@" href="@a/href~1@" title="@a/title~1@">¶</a>'),
        (6, 'Sample Class'),
        (7, '¶'),
        (8, 'A simple sample class for testing.'),
        (9, 'See if this is another sentence.'),
        (10, '&#169; 2024 Sample Footer')
    ]

    expected_extended_segments = {'TRANSLATABLE': {'segment': '', 'attributes': []}, 'TRANSLATABLE/p-0': {'segment': '\np', 'attributes': [], 'children': [{'TRANSLATABLE/p-0': {'segment': 'See if this is a sentence.', 'attributes': []}}]}, 'TRANSLATABLE/html-0': {'segment': 'html', 'attributes': [('html/lang~0', 'en'), ('meta/charset~0', 'utf-8'), ('meta/name~0', 'viewport'), ('meta/content~0', 'width=device-width, initial-scale=1.0'), ('link/rel~0', 'stylesheet'), ('link/type~0', 'text/css'), ('link/href~0', '_static/sample.css'), ('div/class~0', 'document'), ('div/class~1', 'documentwrapper'), ('div/class~2', 'bodywrapper'), ('div/class~3', 'body'), ('div/role~0', 'main'), ('section/id~0', 'sample-section'), ('a/class~0', 'headerlink'), ('a/href~0', '#sample-section'), ('a/title~0', 'Link to this heading'), ('div/class~4', 'toctree-wrapper compound'), ('section/id~1', 'sample-subsection'), ('a/class~1', 'headerlink'), ('a/href~1', '#sample-subsection'), ('a/title~1', 'Link to this heading'), ('dl/class~0', 'py class'), ('dl/id~0', 'sample-class'), ('dt/class~0', 'sig sig-object py'), ('dt/id~0', 'sample-id'), ('span/class~0', 'sig-name descname'), ('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition'), ('div/class~5', 'footer')], 'children': [{'TRANSLATABLE/html-0': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/head-0': {'segment': 'head\n    ', 'attributes': [('meta/charset~0', 'utf-8'), ('meta/name~0', 'viewport'), ('meta/content~0', 'width=device-width, initial-scale=1.0'), ('link/rel~0', 'stylesheet'), ('link/type~0', 'text/css'), ('link/href~0', '_static/sample.css')], 'children': [{'TRANSLATABLE/html-0/head-0': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/head-0/meta-0': {'segment': 'meta', 'attributes': [('meta/charset~0', 'utf-8')], 'children': [{'TRANSLATABLE/html-0/head-0/meta-0': {'segment': '', 'attributes': []}}]}, 'TRANSLATABLE/html-0/head-0/meta-1': {'segment': 'meta', 'attributes': [('meta/name~0', 'viewport'), ('meta/content~0', 'width=device-width, initial-scale=1.0')], 'children': [{'TRANSLATABLE/html-0/head-0/meta-1': {'segment': '', 'attributes': []}}]}, 'TRANSLATABLE/html-0/head-0/title-0': {'segment': 'title', 'attributes': [], 'children': [{'TRANSLATABLE/html-0/head-0/title-0': {'segment': 'Sample HTML', 'attributes': []}}]}, 'TRANSLATABLE/html-0/head-0/link-0': {'segment': 'link', 'attributes': [('link/rel~0', 'stylesheet'), ('link/type~0', 'text/css'), ('link/href~0', '_static/sample.css')], 'children': [{'TRANSLATABLE/html-0/head-0/link-0': {'segment': '', 'attributes': []}}]}}]}, 'TRANSLATABLE/html-0/body-0': {'segment': 'body\n    ', 'attributes': [('div/class~0', 'document'), ('div/class~1', 'documentwrapper'), ('div/class~2', 'bodywrapper'), ('div/class~3', 'body'), ('div/role~0', 'main'), ('section/id~0', 'sample-section'), ('a/class~0', 'headerlink'), ('a/href~0', '#sample-section'), ('a/title~0', 'Link to this heading'), ('div/class~4', 'toctree-wrapper compound'), ('section/id~1', 'sample-subsection'), ('a/class~1', 'headerlink'), ('a/href~1', '#sample-subsection'), ('a/title~1', 'Link to this heading'), ('dl/class~0', 'py class'), ('dl/id~0', 'sample-class'), ('dt/class~0', 'sig sig-object py'), ('dt/id~0', 'sample-id'), ('span/class~0', 'sig-name descname'), ('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition'), ('div/class~5', 'footer')], 'children': [{'TRANSLATABLE/html-0/body-0': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/p-1': {'segment': 'p', 'attributes': [], 'children': [{'TRANSLATABLE/html-0/body-0/p-1': {'segment': 'See if this is a sentence.', 'attributes': []}}]}, 'TRANSLATABLE/html-0/body-0/div-0': {'segment': 'div', 'attributes': [('div/class~0', 'document'), ('div/class~1', 'documentwrapper'), ('div/class~2', 'bodywrapper'), ('div/class~3', 'body'), ('div/role~0', 'main'), ('section/id~0', 'sample-section'), ('a/class~0', 'headerlink'), ('a/href~0', '#sample-section'), ('a/title~0', 'Link to this heading'), ('div/class~4', 'toctree-wrapper compound'), ('section/id~1', 'sample-subsection'), ('a/class~1', 'headerlink'), ('a/href~1', '#sample-subsection'), ('a/title~1', 'Link to this heading'), ('dl/class~0', 'py class'), ('dl/id~0', 'sample-class'), ('dt/class~0', 'sig sig-object py'), ('dt/id~0', 'sample-id'), ('span/class~0', 'sig-name descname'), ('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/div-0/div-1': {'segment': 'div', 'attributes': [('div/class~1', 'documentwrapper'), ('div/class~2', 'bodywrapper'), ('div/class~3', 'body'), ('div/role~0', 'main'), ('section/id~0', 'sample-section'), ('a/class~0', 'headerlink'), ('a/href~0', '#sample-section'), ('a/title~0', 'Link to this heading'), ('div/class~4', 'toctree-wrapper compound'), ('section/id~1', 'sample-subsection'), ('a/class~1', 'headerlink'), ('a/href~1', '#sample-subsection'), ('a/title~1', 'Link to this heading'), ('dl/class~0', 'py class'), ('dl/id~0', 'sample-class'), ('dt/class~0', 'sig sig-object py'), ('dt/id~0', 'sample-id'), ('span/class~0', 'sig-name descname'), ('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2': {'segment': 'div', 'attributes': [('div/class~2', 'bodywrapper'), ('div/class~3', 'body'), ('div/role~0', 'main'), ('section/id~0', 'sample-section'), ('a/class~0', 'headerlink'), ('a/href~0', '#sample-section'), ('a/title~0', 'Link to this heading'), ('div/class~4', 'toctree-wrapper compound'), ('section/id~1', 'sample-subsection'), ('a/class~1', 'headerlink'), ('a/href~1', '#sample-subsection'), ('a/title~1', 'Link to this heading'), ('dl/class~0', 'py class'), ('dl/id~0', 'sample-class'), ('dt/class~0', 'sig sig-object py'), ('dt/id~0', 'sample-id'), ('span/class~0', 'sig-name descname'), ('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3': {'segment': 'div', 'attributes': [('div/class~3', 'body'), ('div/role~0', 'main'), ('section/id~0', 'sample-section'), ('a/class~0', 'headerlink'), ('a/href~0', '#sample-section'), ('a/title~0', 'Link to this heading'), ('div/class~4', 'toctree-wrapper compound'), ('section/id~1', 'sample-subsection'), ('a/class~1', 'headerlink'), ('a/href~1', '#sample-subsection'), ('a/title~1', 'Link to this heading'), ('dl/class~0', 'py class'), ('dl/id~0', 'sample-class'), ('dt/class~0', 'sig sig-object py'), ('dt/id~0', 'sample-id'), ('span/class~0', 'sig-name descname'), ('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0': {'segment': 'section', 'attributes': [('section/id~0', 'sample-section'), ('a/class~0', 'headerlink'), ('a/href~0', '#sample-section'), ('a/title~0', 'Link to this heading'), ('div/class~4', 'toctree-wrapper compound'), ('section/id~1', 'sample-subsection'), ('a/class~1', 'headerlink'), ('a/href~1', '#sample-subsection'), ('a/title~1', 'Link to this heading'), ('dl/class~0', 'py class'), ('dl/id~0', 'sample-class'), ('dt/class~0', 'sig sig-object py'), ('dt/id~0', 'sample-id'), ('span/class~0', 'sig-name descname'), ('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/h1-0': {'segment': 'h1', 'attributes': [('a/class~0', 'headerlink'), ('a/href~0', '#sample-section'), ('a/title~0', 'Link to this heading')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/h1-0': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/h1-0/a-0': {'segment': 'a', 'attributes': [('a/class~0', 'headerlink'), ('a/href~0', '#sample-section'), ('a/title~0', 'Link to this heading')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/h1-0/a-0': {'segment': '¶', 'attributes': []}}]}}]}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/div-4': {'segment': 'div', 'attributes': [('div/class~4', 'toctree-wrapper compound')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/div-4': {'segment': '', 'attributes': []}}]}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1': {'segment': 'section', 'attributes': [('section/id~1', 'sample-subsection'), ('a/class~1', 'headerlink'), ('a/href~1', '#sample-subsection'), ('a/title~1', 'Link to this heading'), ('dl/class~0', 'py class'), ('dl/id~0', 'sample-class'), ('dt/class~0', 'sig sig-object py'), ('dt/id~0', 'sample-id'), ('span/class~0', 'sig-name descname'), ('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/h2-0': {'segment': 'h2', 'attributes': [('a/class~1', 'headerlink'), ('a/href~1', '#sample-subsection'), ('a/title~1', 'Link to this heading')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/h2-0': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/h2-0/a-1': {'segment': 'a', 'attributes': [('a/class~1', 'headerlink'), ('a/href~1', '#sample-subsection'), ('a/title~1', 'Link to this heading')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/h2-0/a-1': {'segment': '¶', 'attributes': []}}]}}]}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0': {'segment': 'dl', 'attributes': [('dl/class~0', 'py class'), ('dl/id~0', 'sample-class'), ('dt/class~0', 'sig sig-object py'), ('dt/id~0', 'sample-id'), ('span/class~0', 'sig-name descname'), ('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0/dt-0': {'segment': 'dt', 'attributes': [('dt/class~0', 'sig sig-object py'), ('dt/id~0', 'sample-id'), ('span/class~0', 'sig-name descname'), ('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0/dt-0': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0/dt-0/span-0': {'segment': 'span', 'attributes': [('span/class~0', 'sig-name descname')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0/dt-0/span-0': {'segment': 'Sample Class', 'attributes': []}}]}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0/dt-0/a-2': {'segment': 'a', 'attributes': [('a/class~2', 'headerlink'), ('a/href~2', '#sample-id'), ('a/title~2', 'Link to this definition')], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0/dt-0/a-2': {'segment': '¶', 'attributes': []}}]}}]}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0/dd-0': {'segment': 'dd\n                    ', 'attributes': [], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0/dd-0': {'segment': '', 'attributes': []}, 'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0/dd-0/p-2': {'segment': 'p', 'attributes': [], 'children': [{'TRANSLATABLE/html-0/body-0/div-0/div-1/div-2/div-3/section-0/section-1/dl-0/dd-0/p-2': {'segment': 'A simple sample class for testing.', 'attributes': []}}]}}]}}]}}]}}]}}]}}]}}]}}]}, 'TRANSLATABLE/html-0/body-0/p-3': {'segment': 'p', 'attributes': [], 'children': [{'TRANSLATABLE/html-0/body-0/p-3': {'segment': 'See if this is another sentence.', 'attributes': []}}]}, 'TRANSLATABLE/html-0/body-0/div-5': {'segment': 'div', 'attributes': [('div/class~5', 'footer')], 'children': [{'TRANSLATABLE/html-0/body-0/div-5': {'segment': '&#169; 2024 Sample Footer', 'attributes': []}}]}}]}}]}}
    
    def normalize_dict(d):
        """
        Recursively sort lists within the dictionary.
        """
        if isinstance(d, dict):
            return {k: normalize_dict(v) for k, v in sorted(d.items())}
        elif isinstance(d, list):
            return sorted(normalize_dict(x) for x in d)
        else:
            return d

    assert template.strip() == expected_template.strip()
    assert sorted(attributes) == sorted(expected_attributes)
    assert post_editing_segments == expected_post_editing_segments
    assert normalize_dict(extended_segments) == normalize_dict(expected_extended_segments)


# `Tokenizer.global_attribute_list` must be per-instance, not a shared
# class attribute, so two Tokenizer objects processing different input in
# the same process cannot see each other's state.
def test_tokenizer_instances_do_not_share_global_attribute_list():
    a = Tokenizer()
    b = Tokenizer()
    # The two instances must not be the same list object even before any
    # tokenize() call — a class attribute would give us the same id().
    assert a.global_attribute_list is not b.global_attribute_list
    # Mutating one must not affect the other.
    a.global_attribute_list.append(('sentinel', 'A'))
    assert ('sentinel', 'A') not in b.global_attribute_list
    # And a tokenize() call on `b` must not inherit `a`'s mutation.
    b.tokenize('<p>hello</p>')
    assert ('sentinel', 'A') not in b.global_attribute_list


# Each generate_svg() call must own its own node-id Counter so two
# successive calls on identical trees produce identical SVG — a shared
# counter would leak state across calls in a long-running process
# (e.g., a FastAPI worker).
def test_generate_svg_restarts_node_ids_per_call():
    node = TreeNode(
        'SINGLE',
        'root',
        ['<tag>', '</tag>'],
        children=[TreeNode('TRANSLATABLE', 'child_value', '')],
    )
    first = node.generate_svg()
    second = node.generate_svg()
    # Two back-to-back calls on the same tree must produce identical
    # SVG — the node id counter must not leak across calls.
    assert first == second
    # And the root node must always be id=node0, never id=nodeN for
    # some N>0 accumulated from prior calls.
    assert 'id="node0"' in first


# ---------------------------------------------------------------------------
# Direct unit tests for the Rust-backed API surface. These pin the
# per-method contracts so a regression in any one primitive fails with a
# narrow, readable error instead of a 300-line dict diff.
# ---------------------------------------------------------------------------


def test_tree_node_from_dict_round_trip():
    """from_dict must reconstruct a tree that survives build_full_value."""
    d = {
        'node_type': 'SINGLE',
        'value': 'root',
        'tag': ['<p>', '</p>'],
        'attributes': [['class', 'foo'], ['id', 'bar']],
        'children': [
            {
                'node_type': 'TRANSLATABLE',
                'value': 'hello world',
                'tag': [],
            },
        ],
    }
    node = TreeNode.from_dict(d)
    assert node.node_type == 'SINGLE'
    assert node.value == 'root'
    assert node.tag == ['<p>', '</p>']
    assert node.get_attributes() == [('class', 'foo'), ('id', 'bar')]
    assert len(node.children) == 1
    assert node.children[0].node_type == 'TRANSLATABLE'
    assert node.children[0].value == 'hello world'
    assert node.build_full_value() == '<p>hello world</p>'
    assert node.build_pattern() == '<p>TRANSLATABLE</p>'


def test_tree_node_from_dict_missing_field_raises():
    with pytest.raises(InvalidNodeDictError):
        TreeNode.from_dict({'node_type': 'SINGLE'})


def test_tree_node_from_dict_wrong_tag_type_raises():
    with pytest.raises(InvalidNodeDictError):
        TreeNode.from_dict(
            {'node_type': 'SINGLE', 'value': 'x', 'tag': 'notalist'}
        )


def test_tree_node_build_pattern_single_and_nested():
    inner = TreeNode('TRANSLATABLE', 'hi', [])
    node = TreeNode('SINGLE', 'v', ['<b>', '</b>'], children=[inner])
    # Leaves render as the literal string 'TRANSLATABLE'; Single nodes
    # fence child output with their open/close tag pair.
    assert node.build_pattern() == '<b>TRANSLATABLE</b>'
    assert inner.build_pattern() == 'TRANSLATABLE'


def test_tree_node_strip_spaces_leading_trailing():
    # strip_spaces is pure on (self-independent) text — any node will do.
    node = TreeNode('TRANSLATABLE', '', [])

    # Leading spaces become `{sp}` placeholders, trailing "spaces+newlines"
    # are stripped into the third tuple slot.
    assert node.strip_spaces('   hello world  \n\n') == (
        'hello world', '{sp}{sp}{sp}', '{sp}{sp}\n\n'
    )
    # No leading/trailing whitespace: both placeholders empty.
    assert node.strip_spaces('nochange') == ('nochange', '', '')
    # Leading only.
    assert node.strip_spaces(' leading only') == ('leading only', '{sp}', '')
    # Trailing newline only.
    assert node.strip_spaces('trailing \n') == ('trailing', '', '{sp}\n')


def test_tree_node_generate_placeholders_for_segments_simple():
    # A single SINGLE node wrapping one TRANSLATABLE leaf produces exactly
    # one post-editing segment and a template with one %%1%% placeholder.
    child = TreeNode('TRANSLATABLE', 'child_value', [])
    node = TreeNode('SINGLE', 'value', ['<tag>', '</tag>'], children=[child])

    end_index, template, segments = node.generate_placeholders_for_segments('')

    assert template == '<tag>%%1%%</tag>'
    assert segments == [(1, 'child_value')]
    # `end_index` is the highest segment index actually consumed, which is
    # 1 here because exactly one %%N%% placeholder was emitted.
    assert end_index == 1


def test_tree_node_extract_extended_segments_leaf():
    node = TreeNode('TRANSLATABLE', 'hi there', [])
    ext = node.extract_extended_segments()
    # Leaf shape: a single-entry dict keyed by tag-path with no children.
    assert 'TRANSLATABLE' in ext
    assert ext['TRANSLATABLE']['segment'] == 'hi there'
    assert ext['TRANSLATABLE']['attributes'] == []
    assert 'children' not in ext['TRANSLATABLE']


def test_segment_tree_from_tree_constructor():
    # Hand-build a tree then wrap it in a SegmentTree without going through
    # a token list. `from_tree` is a staticmethod that takes an existing
    # TreeNode and exposes it via the `.tree` getter.
    inner = TreeNode('TRANSLATABLE', 'payload', [])
    node = TreeNode('SINGLE', 'v', ['<p>', '</p>'], children=[inner])
    st = SegmentTree.from_tree(node)
    assert st.tree.node_type == 'SINGLE'
    assert st.tree.value == 'v'
    assert st.tree.tag == ['<p>', '</p>']
    assert len(st.tree.children) == 1
    assert st.tree.children[0].node_type == 'TRANSLATABLE'


def test_segment_tree_rejects_non_dict_token():
    with pytest.raises(TypeError):
        SegmentTree(['not a dict'])  # type: ignore[list-item]


def test_segment_tree_rejects_token_missing_required_fields():
    with pytest.raises(InvalidTokenError):
        SegmentTree([{'text': 'no class here'}])


# ---------------------------------------------------------------------------
# Binary / binary-template coverage. `test_complex_html_segment` only
# exercises the SINGLE/MULTIPLE/TRANSLATABLE path. A plain sentence with a
# BREAK (period+space) token in it produces a BINARY tree — make sure that
# branch of split_token_list still works end-to-end.
# ---------------------------------------------------------------------------


def test_binary_split_from_sentence():
    tokenizer = Tokenizer()
    tokens = tokenizer.tokenize('Hello. World.')
    tree = SegmentTree(tokens).tree

    # A BREAK on `. ` splits into left=Multiple[TRANSLATABLE 'Hello'] and
    # right=Binary{operator='.'} for the trailing sentence.
    assert tree.node_type == 'BINARY'
    assert tree.value == '. '

    # The reconstructed text must equal the input exactly.
    assert tree.build_full_value() == 'Hello. World.'

    # The pattern collapses every TRANSLATABLE leaf to the literal string.
    assert tree.build_pattern() == 'TRANSLATABLE. TRANSLATABLE.TRANSLATABLE'

    # generate_placeholders_for_segments must still emit a template + segs.
    _, template, segments = tree.generate_placeholders_for_segments('')
    seg_texts = [t for _, t in segments]
    # Sentence-level segments keep their trailing punctuation.
    assert 'Hello.' in seg_texts
    assert 'World.' in seg_texts


def test_unmatched_and_nesting_errors_captured():
    tokenizer = Tokenizer()
    tokenizer.tokenize('<p>unclosed')
    unmatched = tokenizer.get_unmatched_tags()
    nesting = tokenizer.get_nesting_errors()
    # An unclosed `<p>` must show up in both reports.
    assert any(
        t.get('text') == '<p>' and t.get('label') == 'p'
        for t in unmatched
    ), f'unmatched missing <p>: {unmatched}'
    assert any(
        t.get('text') == '<p>' and t.get('label') == 'p'
        for t in nesting
    ), f'nesting missing <p>: {nesting}'


# ---------------------------------------------------------------------------
# Empty-input corner case. The tokenizer must produce zero tokens, and
# SegmentTree must still build a valid (DUMMY) tree that every downstream
# method handles without raising. This is the degenerate base case the
# end-to-end tests never exercise.
# ---------------------------------------------------------------------------


def test_empty_input_parses_to_dummy_tree():
    tokenizer = Tokenizer()
    tokens = tokenizer.tokenize('')
    assert tokens == []
    assert tokenizer.get_unmatched_tags() == []
    assert tokenizer.get_nesting_errors() == []

    tree = SegmentTree(tokens).tree
    # Zero tokens collapses to a single DUMMY leaf — a sentinel node with
    # no value, no tag, no children.
    assert tree.node_type == 'DUMMY'
    assert tree.value == ''
    assert tree.tag == []
    assert len(tree.children) == 0

    # Every downstream primitive must tolerate the empty tree without
    # raising. build_full_value is the round-trip check; template/segments
    # is the translation path; extract_* is the analysis path.
    assert tree.build_full_value() == ''
    assert tree.build_pattern() == 'TRANSLATABLE'
    _, template, segments = tree.generate_placeholders_for_segments('')
    assert template == ''
    assert segments == []
    assert tree.extract_segments() == [{'TRANSLATABLE': ''}]
    assert tree.extract_extended_segments() == {
        'TRANSLATABLE': {'segment': '', 'attributes': []}
    }
    assert tree.get_attributes() == []


def test_well_formed_input_has_no_unmatched_or_nesting_errors():
    tokenizer = Tokenizer()
    tokenizer.tokenize('<p>hello</p>')
    assert tokenizer.get_unmatched_tags() == []
    assert tokenizer.get_nesting_errors() == []


if __name__ == '__main__':
    pytest.main()
