# Interactive Picker

The interactive picker lets you **fuzzy-search your test names and select specific tests to run**. Instead of remembering exact test names or filter patterns, you get a searchable list right in your terminal.

---

## Usage

```bash
testx pick
```

You can also pass extra arguments to the test runner:

```bash
testx pick -- --verbose
```

---

## How it works

1. testx runs your test suite once in "discovery" mode to collect all test names
2. It displays up to 20 tests, numbered for easy selection
3. You search, select, and testx runs only the tests you chose

---

## Interaction guide

### Search by name

Start typing to fuzzy-filter the list. Matched characters are highlighted:

```
> auth
  1. test_auth_login
  2. test_auth_logout
  3. test_authorization_header

Select (numbers, or Enter for all):
```

The search is fuzzy, so you don't need exact matches — typing `alog` would still match `test_auth_login` and `test_auth_logout`.

### Select by number

Type comma-separated numbers to pick specific tests:

```
Select (numbers, or Enter for all): 1,3
```

This runs only `test_auth_login` and `test_authorization_header`.

### Select all matches

Press Enter without typing any numbers to run **all** currently visible tests.

### Cancel

Type `q` to cancel and exit without running anything.

---

## How fuzzy scoring works

Tests are ranked by match quality, so the best matches appear first:

- **Exact substring matches** score highest (typing `login` matches `test_login` strongly)
- **Start-of-name matches** are preferred (typing `test` favors `test_hello` over `my_test`)
- **Consecutive characters** score higher (typing `log` in `login` beats `l...o...g`)
- **Word boundary matches** get a bonus — characters after `_`, `:`, `.`, `/` are weighted more
- **Shorter names** are preferred over longer ones when match quality is otherwise equal

---

## Tips

- Use `testx pick` when you're debugging a specific test and can't remember the exact name
- Combine with `--` to pass extra flags: `testx pick -- -v` runs your selected tests with the test runner's verbose flag
- The picker works with any language/framework testx supports
