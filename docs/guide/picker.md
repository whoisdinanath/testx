# Interactive Picker

Fuzzy-search your test names and pick specific tests to run.

## Usage

```bash
testx pick
testx pick -- --verbose
```

## How it works

1. testx runs the test suite once to discover all test names
2. Displays up to 20 tests with numbered indices
3. You can select tests by number or search by name

## Interaction

### Select by number

Type comma-separated numbers to select specific tests:

```
1,3,5
```

### Search by name

Type any text to fuzzy-filter the list. Matched characters are highlighted:

```
> auth
  1. test_**auth**_login
  2. test_**auth**_logout
  3. test_**auth**orization_header

Select (numbers, or Enter for all): 1,2
```

### Select all matches

Press Enter without typing numbers to select all currently visible tests.

### Cancel

Type `q` to cancel and exit.

## Fuzzy scoring

Tests are ranked by match quality:

- Exact substring matches score highest
- Matches at the start of the name are preferred
- Consecutive matched characters score higher
- Matches after word boundaries (`_`, `:`, `.`, `/`) get a bonus
- Shorter test names are preferred over longer ones
