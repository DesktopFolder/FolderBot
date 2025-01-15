from typing import Callable, Optional
from twitchio import Chatter, PartialChatter
from twitchio.ext import commands
from daemon import data_dir, datafile, seconds_since_update, duration_since_update
from query import DATA, PacemanObject, DATA_SORTED, ALL_SPLITS, USEFUL_DATA, td
from sys import argv


def partition(l: list, p: Callable):
    has_it = [x for x in l if p(x)]
    nopers = [x for x in l if not p(x)]
    return (has_it, nopers)


def pctg(a, b):
    return f'{round(100 * b / a, 2)}'

def default_file(filename: str, data: str):
    try:
        return open(filename).read()
    except Exception:
        return data

class Bot(commands.Bot):

    def __init__(self, prefix='?'):
        import json
        self.prefix = prefix
        default_configuration = """
        {
            "folderbot": {
                "name": "folderbot"
            },
            "desktopfolder": {
                "name": "desktopfolder"
            },
            "snakezy": {
                "name": "snakezy"
            },
            "doypingu": {
                "name": "doypingu"
            },
            "queenkac": {
                "name": "queenkac"
            },
            "cooshw": {
                "name": "cooshw",
                "player": "coosh02"
            }
        }
        """
        self.configuration: dict[str, dict] = json.loads(default_file(datafile("folderbot.json"), default_configuration))

        super().__init__(token=open('auth/ttg-access.txt').read().strip(), prefix=prefix, initial_channels=[k for k in self.configuration.keys()])

    def save(self):
        with open(datafile("folderbot.json"), "w") as file:
            import json
            json.dump(self.configuration, file, indent=2)

    async def event_ready(self):
        # Notify us when everything is ready!
        # We are logged in and ready to chat and use commands...
        print(f'Logged in as | {self.nick}')
        print(f'User id is | {self.user_id}')

    ########################################################################################
    ############################ Methods to send generic strings ###########################
    ########################################################################################
    @commands.command()
    async def help(self, ctx: commands.Context, page = 1): ##### help
        helpers = [
            "AA Paceman extension: ?average [splitname] [playername] -> average split for a player, ?help 2 -> Bot configuration help, ?help 3 -> Bot metainformation help",
            "(help 2) ?join -> Join the bot to your channel, ?setplayer -> Set the default player for this channel",
            '(help 3) ?info -> Metadata on bot status, ?botdiscord -> Server with bot information, ?about -> Credits'
        ]
        p = page - 1
        if p < 0 or p >= len(helpers):
            return await ctx.send(f"Page number is out of bounds (maximum: {len(helpers)})")
        await ctx.send(helpers[p])
    @commands.command()
    async def botdiscord(self, ctx: commands.Context): ##### bot discord
        await ctx.send("For to-do list & feature requests: https://discord.gg/NSp5t3wfBP")
    @commands.command()
    async def about(self, ctx: commands.Context): ##### about
        await ctx.send("Made by DesktopFolder. Uses stats from Jojoe's Paceman AA API. Uses local caching to reduce API calls.")
    @commands.command()
    async def info(self, ctx: commands.Context):  ##### info
        dur0 = duration_since_update()
        data = DATA_SORTED()
        dur = duration_since_update()
        infos = [f'Time since update: {dur}.']
        if dur0 != dur:
            infos.append(f'({dur0} before this command)')
        infos.append(f'{len(data)} known AA runs.')
        last_nether = PacemanObject(data[0])
        if last_nether.get('nether') is not None:
            infos.append(f'Last nether: {last_nether.get_str("nether")} by {last_nether.player}.')
        await ctx.send(' '.join(infos))

    ########################################################################################
    ############################# Methods to configure the bot #############################
    ########################################################################################
    @commands.command()
    async def setplayer(self, ctx: commands.Context, playername: str):
        if not isinstance(ctx.author, Chatter):
            return await ctx.send('Cannot validate that you are the broadcaster.')
        if not ctx.author.is_broadcaster:
            return await ctx.send('Only the broadcaster can use this command.')
        cn = ctx.channel.name.lower()
        if not cn in self.configuration:
            return await ctx.send('Let me know if you see this.')
        self.configuration[cn]['player'] = playername
        self.save()
        return await ctx.send(f'Set default player to {playername}.')

    @commands.command()
    async def join(self, ctx: commands.Context, agree: str = ""):
        cn = ctx.author.name
        if cn is None:
            return await ctx.send("Name was none; if this issue persists, contact DesktopFolder.")
        if cn in self.configuration:
            return await ctx.send(f"Bot is already joined to {cn}.")
        cn = cn.lower()
        if agree != "agree":
            return await ctx.send(f'This is in development. See {self.prefix}botdiscord for current todos/feature requests. If you agree, and want to join this bot to {cn}, type {self.prefix}join agree')
        self.configuration[cn] = {"name": cn}
        self.save()
        await self.join_channels([cn])
        return await ctx.send(f'Theoretically joined {cn}.')

    @commands.command()
    async def average(self, ctx: commands.Context, splitname: str, playername: Optional[str] = None):
        playername = self.playername(ctx, playername)
        splitname = splitname.lower()
        if not splitname in ALL_SPLITS:
            return await ctx.send(f'{splitname} is not a valid AA split: {ALL_SPLITS}')
        pcs = [p for p in USEFUL_DATA() if p.filter(split=splitname, player=playername)]
        if len(pcs) == 0:
            return await ctx.send(f'{playername} has no known {splitname} AA splits.')
        await ctx.send(f'Average AA {splitname} for {playername}: {td.average(ts=[pc.get(splitname) for pc in pcs])} (sample: {len(pcs)})')

    @commands.command()
    async def conversion(self, ctx: commands.Context, split1: str, split2: str, playername: Optional[str] = None):
        playername = self.playername(ctx, playername)
        # yikes need to do some refactoring
        split1 = split1.lower()
        split2 = split2.lower()
        for split in [split1, split2]:
            if not split in ALL_SPLITS:
                return await ctx.send(f'{split} is not a valid AA split: {ALL_SPLITS}')

        pcs = [p for p in USEFUL_DATA() if p.filter(split=split1, player=playername)]
        if len(pcs) == 0:
            return await ctx.send(f'{playername} has no known {split1} AA splits.')
        n = len(pcs)
        x = len([p for p in pcs if p.has(split2)])
        await ctx.send(f'{pctg(n, x)}% ({x} / {n}) of {playername}\'s AA {split1} splits lead to starting {split2} splits.')

    @commands.command()
    async def count(self, ctx: commands.Context, split: str, playername: Optional[str] = None):
        playername = self.playername(ctx, playername)
        if not split in ALL_SPLITS:
            return await ctx.send(f'{split} is not a valid AA split: {ALL_SPLITS}')
        if playername == '!total':
            playername = None
        pcs = [p for p in USEFUL_DATA() if p.filter(split=split, player=playername)]
        d = sorted(pcs, key=lambda p: p.get(split) or 0)
        fastest = d[0].get(split)
        fastest_name = d[0].player
        seed = f'{len(pcs)} known {split} times. Fastest: {td(fastest)}'
        if playername is None:
            return await ctx.send(f'There are {seed} (by {fastest_name})')
        else:
            return await ctx.send(f'{playername} has {seed}')

    def data_filtered(self, ctx: commands.Context, split: Optional[str], playername: Optional[str] = None):
        if playername == None:
            src = USEFUL_DATA()
        else:
            src = [p for p in USEFUL_DATA() if p.filter(player=playername)]
        src = [p for p in src if p.filter(split=split)]
        return src

    @commands.command()
    async def countlt(self, ctx: commands.Context, split: str, time: str, playername: Optional[str] = None):
        playername = self.playername(ctx, playername)
        if not split in ALL_SPLITS:
            return await ctx.send(f'{split} is not a valid AA split: {ALL_SPLITS}')
        if playername == '!total':
            playername = None
        pcs = self.data_filtered(ctx, split, playername)
        pcs = [t for t in [p.get(split) for p in pcs] if t is not None]
        try:
            maximum = td(time)
        except Exception:
            return await ctx.send(f'Invalid time {time}, follow format hh:mm:ss (hours/seconds optional, but seconds required for hours')
        pcs = [t for t in pcs if t <= maximum.src]

        if playername is None:
            return await ctx.send(f'There are {len(pcs)} {split} times faster than {maximum}.')
        else:
            return await ctx.send(f'{playername} has {len(pcs)} {split} times faster than {maximum}.')

    @commands.command()
    async def countgt(self, ctx: commands.Context, split: str, time: str, playername: Optional[str] = None):
        playername = self.playername(ctx, playername)
        if not split in ALL_SPLITS:
            return await ctx.send(f'{split} is not a valid AA split: {ALL_SPLITS}')
        if playername == '!total':
            playername = None
        pcs = self.data_filtered(ctx, split, playername)
        pcs = [t for t in [p.get(split) for p in pcs] if t is not None]
        try:
            minimum = td(time)
        except Exception:
            return await ctx.send(f'Invalid time {time}, follow format hh:mm:ss (hours/seconds optional, but seconds required for hours')
        pcs = [t for t in pcs if t > minimum.src]

        if playername is None:
            return await ctx.send(f'There are {len(pcs)} {split} times slower than {minimum}.')
        else:
            return await ctx.send(f'{playername} has {len(pcs)} {split} times slower than {minimum}.')

    def playername(self, ctx: commands.Context, playername: Optional[str] = None) -> str:
        if playername is not None:
            return playername
        cn = ctx.channel.name.lower()
        if cn not in self.configuration:
            return 'If you see this, please tell DesktopFolder'
        c = self.configuration[cn]
        if 'player' in c:
            return c['player']
        return cn

    @commands.command()
    async def bastion_breakdown(self, ctx: commands.Context, playername: Optional[str] = None):
        playername = self.playername(ctx, playername)
        pcs = [p for p in USEFUL_DATA() if p.filter(split='nether', player=playername)]
        if len(pcs) == 0:
            return await ctx.send(f'{playername} has no known AA nethers.')

        def pctgwith(l: list[PacemanObject]):
            n = len(l)
            x = len([p for p in l if p.has('bastion')])
            return pctg(n, x)

        def writer(l: list[PacemanObject], s: str):
            if not l:
                return ''
            return f'Conversion for {s} nethers: {pctgwith(l)}% ({len(l)})'

        sub_330, pcs = partition(pcs, lambda p: p.get('nether').total_seconds() < 60 * 3 + 30)
        sub_400, pcs = partition(pcs, lambda p: p.get('nether').total_seconds() < 60 * 4)
        sub_430, pcs = partition(pcs, lambda p: p.get('nether').total_seconds() < 60 * 4 + 30)
        sub_500, pcs = partition(pcs, lambda p: p.get('nether').total_seconds() < 60 * 5)
        brk = ' | '.join([x for x in [
                writer(sub_330, '< 3:30'),
                writer(sub_400, '3:30-4:00'),
                writer(sub_430, '4:00-4:30'),
                writer(sub_500, '4:30-5:00'),
                writer(pcs, '> 5:00'),
            ]
            if x != '' 
        ])
        await ctx.send(f'Bastion conversion breakdown for {playername}: {brk}')


if __name__ == '__main__':
    args = argv[1:]
    if 'test' in args:
        bot = Bot(prefix='%')
    else:
        bot = Bot()
    bot.run()
