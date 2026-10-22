"""Checks a list of candidate names against the registries a showcase repo and an app would collide with.

Usage: python3 -I validate-names.py name1 name2 ...
Prints one row per name; a name survives when npm, PyPI, crates and Homebrew are free, the App Store search
returns no app whose name starts with the candidate, and GitHub has few repos carrying the word.
"""
import json, subprocess, sys, time, urllib.parse, urllib.request


def fetch_status(url):
    req = urllib.request.Request(url, headers={'User-Agent': 'name-check/1.0'})
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, b''
    except Exception:
        return -1, b''


def npm_free(name):
    status, _ = fetch_status(f'https://registry.npmjs.org/{name}')
    return status == 404


def pypi_free(name):
    status, _ = fetch_status(f'https://pypi.org/pypi/{name}/json')
    return status == 404


def crates_free(name):
    status, _ = fetch_status(f'https://crates.io/api/v1/crates/{name}')
    return status == 404


def brew_free(name):
    s1, _ = fetch_status(f'https://formulae.brew.sh/api/formula/{name}.json')
    s2, _ = fetch_status(f'https://formulae.brew.sh/api/cask/{name}.json')
    return s1 == 404 and s2 == 404


def appstore_hits(name):
    q = urllib.parse.quote(name)
    status, body = fetch_status(f'https://itunes.apple.com/search?term={q}&entity=software,macSoftware&limit=25')
    if status != 200:
        return '?'
    try:
        results = json.loads(body).get('results', [])
    except Exception:
        return '?'
    exact = [r.get('trackName', '') for r in results if r.get('trackName', '').lower().replace(' ', '').startswith(name.lower())]
    return exact[:3]


def github_count(name):
    try:
        out = subprocess.run(['gh', 'api', f'search/repositories?q={name}+in:name&per_page=1', '--jq', '.total_count'],
                             capture_output=True, text=True, timeout=30)
        return int(out.stdout.strip()) if out.returncode == 0 and out.stdout.strip().isdigit() else -1
    except Exception:
        return -1


def main(names):
    print(f'{"name":<12} {"npm":<5} {"pypi":<5} {"crate":<5} {"brew":<5} {"gh":>6}  appstore')
    for n in names:
        row = (npm_free(n), pypi_free(n), crates_free(n), brew_free(n))
        gh = github_count(n)
        apps = appstore_hits(n)
        flag = lambda b: 'free' if b else 'TAKEN'
        survives = all(row) and gh != -1 and gh <= 40 and apps == []
        print(f'{n:<12} {flag(row[0]):<5} {flag(row[1]):<5} {flag(row[2]):<5} {flag(row[3]):<5} {gh:>6}  {apps if apps else "none"}  {"<- survives" if survives else ""}')
        time.sleep(2.2)


if __name__ == '__main__':
    main(sys.argv[1:])
