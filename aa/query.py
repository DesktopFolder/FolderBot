#!/usr/bin/env python3
from daemon import auto_update, datafile
import json
from collections import defaultdict

class td:
    def __init__(self, t):
        from datetime import timedelta
        assert t is not None
        if isinstance(t, str):
            if ':' not in t:
                minutes = int(t)
                t = timedelta(minutes=minutes)
            elif len(t) > 5: # 00:00 == 5
                hours, minutes, seconds = t.split(':', 2)
                hours = int(hours)
                assert hours < 1000
                assert len(minutes) < 3
                assert len(seconds) < 3
                minutes = int(minutes) + 60 * hours
                # just do the total cause I don't want to check
                # timedelta documentation, lol
                seconds = int(seconds) + 60 * minutes
                t = timedelta(seconds=seconds)
            else:
                minutes, seconds = t.split(':', 1)
                minutes = int(minutes)
                # just do the total cause I don't want to check
                # timedelta documentation, lol
                seconds = int(seconds) + 60 * minutes
                t = timedelta(seconds=seconds)
        self.src: timedelta = t
        self.hours, rem = divmod(self.src.seconds, 3600)
        self.minutes, self.seconds = divmod(rem, 60)

    def __str__(self):
        src = f'{self.minutes:02}:{self.seconds:02}'
        if self.hours > 0:
            return f'{self.hours}:{src}'
        return src

    def total_seconds(self):
        return self.src.total_seconds()

    @staticmethod
    def average(ts: list):
        from datetime import timedelta
        if len(ts) == 0:
            return -1
        l = [t.total_seconds() for t in ts]
        return td(timedelta(seconds=sum(l) // len(l)))

DATAFILE = "aa.json"
"""
  {
    "id": 801,
    "nickname": "Byugoi",
    "uuid": "f2be0e3c-500b-4422-af06-9a84ebee30bc",
    "twitch": "byugoi_",
    "nether": 317181,
    "end": 1402616,
    "elytra": 1715866,
    "credits": 3039966,
    "finish": 15830610,
    "insertTime": "2024-09-25T02:28:50.000Z",
    "worldId": "5b0035d398813c84caf79b4e5803468ab7b52d954246412f7fb5ca45a47be85e",
    "lastUpdated": "2024-09-25T08:04:25.000Z",
    "count": 80,
    "bastion": 446681,
    "fortress": 731781,
    "first_portal": 867731,
    "stronghold": 1124483
  },
"""
# AA_TIME_BARRIER = (1 * 60 + 30) * 1000
# Let's do sub2 instead
AA_TIME_BARRIER = (2 * 60) * 1000
ALL_SPLITS = [
    'nether', 'bastion', 'fortress', 'first_portal', 'stronghold', 'end', 'elytra', 'credits', 'finish'
]

class PacemanObject:
    def __init__(self, d: dict) -> None:
        from datetime import  timedelta
        ms_or = lambda k: timedelta(milliseconds=d[k]) if k in d else None
        self.id = d.get("id")
        self.player = d["nickname"]
        # don't care uuid for now.
        self.twitch_root = d.get("twitch")

        # NOTE: insert_time is when nether was entered.

        self.num_advancements = d.get("count", 0)

        # So much better than before, lol.
        self.splits: dict[str, timedelta] = dict()
        for k, v in d.items():
            if not isinstance(v, int) or v < 1000 * 60:
                continue
            self.splits[k] = timedelta(milliseconds=v)

    def get_str(self, split: str):
        res = self.splits.get(split)
        if res is None:
            return f'(No {split} split found)'
        v = td(res)
        src = f'{v.minutes:02}:{v.seconds:02}'
        if v.hours > 0:
            return f'{v.hours}:{src}'
        return src

    @property
    def twitch(self):
        if self.twitch_root is None:
            return None
        return f'https://twitch.tv/{self.twitch_root}'

    def is_player(self, other: str):
        return self.player.lower() == other.lower()

    def has(self, split: str):
        return split in self.splits

    def get(self, split: str):
        return self.splits.get(split)

    def filter(self, split=None, player=None):
        # meh quick and dirty it works I guess
        if split is not None and not self.has(split):
            return False
        if player is not None and not self.is_player(player):
            return False
        return True


def DATA() -> list[dict]:
    auto_update()
    data = json.load(open(datafile(DATAFILE)))
    return [o for o in data if o.get("nether") > AA_TIME_BARRIER or 'elytra' in o]

# Sorted by INSERT TIME!!!
def DATA_SORTED() -> list[dict]:
    from daemon import duration_since
    return sorted(DATA(), key=lambda d: duration_since(d["insertTime"]).total_seconds())

def USEFUL_DATA() -> list[PacemanObject]:
    return [PacemanObject(d) for d in DATA_SORTED()]


def unique_keys():
    d = defaultdict(lambda *args: 0)
    for o in DATA():
        for k in o.keys():
            d[k] += 1
    return d

def all_values(key: str, postprocess=None, paired=False, pred=None):
    values = [(o[key], o) for o in DATA() if key in o]
    if pred is not None:
        values = [(val, o) for val, o in values if pred(o)]
    if postprocess is not None:
        values = [(postprocess(val), o) for val, o in values]
    if not paired:
        values = [val for val, _ in values]
    return values

def average_by(key: str, by: str):
    from datetime import timedelta
    d = defaultdict(lambda: list())
    for o in DATA():
        if key not in o:
            continue
        v = o[key]
        k = o[by]
        d[k].append(v)
    after = list()
    for k, v in d.items():
        after.append((k, len(v), sum(v) / len(v)))
    after = sorted(after, key=lambda o: o[2])
    for name, played, avg in after:
        print("Player:", name, "-", str(timedelta(milliseconds=int(avg))), f"({played})")


def pretty_with(key: str, sort=None, **kwargs):
    values = all_values(key, **kwargs, paired=True)
    res = list()
    for val, o in values:
        as_json = json.dumps(o, indent=2)
        res.append((f"{key}: {val}\n{as_json}", val))
    if sort:
        res = sorted(res, key=lambda v: v[1])
    return "\n\n".join([s for s, _ in res])

def pretty_ms(v: int):
    from datetime import timedelta
    return str(timedelta(milliseconds=v))

def analyze(p: str):
    for x in DATA():
        if x['nickname'].lower() == p.lower():
            print(x)

def main(args: list[str]):
    print(unique_keys())
    print(pretty_with("finish", postprocess=pretty_ms))
    average_by("nether", "nickname")
    # print(pretty_with("nether", pred=lambda o: o["nickname"] == "Feinberg", sort=True, postprocess=pretty_ms))

if __name__ == "__main__":
    from sys import argv
    main(argv[1:])
