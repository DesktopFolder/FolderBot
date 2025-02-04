from datetime import timedelta
from typing import Callable, Optional
from twitchio import Chatter, Message, PartialChatter
from twitchio.ext import commands
from daemon import data_dir, datafile, seconds_since_update, duration_since_update
from query import DATA, PacemanObject, DATA_SORTED, ALL_SPLITS, USEFUL_DATA, td
from sys import argv


async def do_send(ctx: commands.Context, s: str):
    return await ctx.send(s.lstrip('?!/'))


class ParseResult:
    def __init__(self, split: Optional[str], player: str, time: Optional[td]) -> None:
        self.player = player
        self.split = split
        self.time = time

    def split_str(self):
        if self.split is not None:
            return self.split
        return 'nether'

    def player_str(self):
        if self.is_everyone():
            return 'Playerbase'
        return self.player

    def is_everyone(self):
        return self.player == '!total'

    def tr_str(self):
        if self.time is not None:
            return f' (in the last {self.time})'
        return ''

    def data(self):
        d = USEFUL_DATA(self.split, self.player)
        if self.time is None:
            return d
        ret: list[PacemanObject] = list()
        for o in d:
            match_time_since = o.time_since()
            if match_time_since is None:
                continue
            seconds_since_run = match_time_since.total_seconds()
            if seconds_since_run <= self.time.total_seconds():
                ret.append(o)
        return ret

    async def with_data(self, ctx: commands.Context):
        data = self.data()
        if not data:
            if self.player == '!total':
                if self.time is not None:
                    await do_send(ctx, f'Could not find any {self.split_str()} in the last {self.time} (for any player)')
                else:
                    await do_send(ctx, f'Could not find any instances of the split {self.split_str()}, are you sure it\'s spelled correctly?')
            elif self.time is not None:
                # defined player + time span
                await do_send(ctx, f'Could not find any {self.split_str()} in the last {self.time} for the player {self.player}')
            else:
                # Defined player + no time span
                await do_send(ctx, f'Could not find any {self.split_str()} for the player {self.player}')
            return None
        return data
            

def clean(s: str):
    return ''.join([ch for ch in s if ch.isalnum() or ch == '_'])


def partition(l: list, p: Callable):
    has_it = [x for x in l if p(x)]
    nopers = [x for x in l if not p(x)]
    return (has_it, nopers)

def btd(time: timedelta):
    return timedelta(seconds=int(time.total_seconds()))

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
        print('Saved data.')

    def add(self, ctx: commands.Context, command: str):
        import random
        cn = ctx.channel.name.lower()
        unknown = '^ unknown' 
        if cn not in self.configuration:
            if unknown not in self.configuration:
                self.configuration['unknown'] = dict()
            loc = self.configuration['unknown']
        else:
            loc = self.configuration[cn]
        cmd = f'command_{command}'
        if cmd not in loc:
            loc[cmd] = 0
        loc[cmd] += 1

        if random.random() < 0.1:
            self.save()


    async def event_ready(self):
        # Notify us when everything is ready!
        # We are logged in and ready to chat and use commands...
        print(f'Logged in as | {self.nick}')
        print(f'User id is | {self.user_id}')

    async def handle_commands(self, message: Message):
        """
        Handles commands from a given message.
        This method checks if the message originates from the same channel
        and if so, it delegates the handling of the commands to the parent class.
        Args:
            message (Message): The message object containing the command to be handled.
        Returns:
            None
        """
        if self.originates_from_same_channel(message):
            await super().handle_commands(message)
    
    def originates_from_same_channel(self, message: Message):
        """
        Determines if a message originates from the same channel.
        This method checks if the message's destination channel ID matches its originating channel ID.
        If the originating channel ID is not present, it assumes the message does not originate from a shared chat.
        Args:
            message (Message): The message object containing tags with channel information.
        Returns:
            bool: True if the message originates from the same channel or if there is no originating channel ID,
              False otherwise.
        """
        dest_channel_id = message.tags.get('room-id') if 'room-id' in message.tags else None
        if dest_channel_id is None:
            return False
        originating_channel_id = message.tags.get('source-room-id') if 'source-room-id' in message.tags else None
        return originating_channel_id is None or originating_channel_id == dest_channel_id


    @commands.command()
    async def quicksave(self, ctx: commands.Context): ##### help
        if not isinstance(ctx.author, Chatter):
            return await do_send(ctx, 'No Chatter instance found.')
        if ctx.author.name.lower() == 'desktopfolder':
            # yeah just me thanks.
            self.save()
            return await do_send(ctx, 'Quicksaved state.')
        else:
            return await do_send(ctx, 'Only the bot maintainer can use this command.')

    ########################################################################################
    ############################ Methods to send generic strings ###########################
    ########################################################################################
    @commands.command()
    async def help(self, ctx: commands.Context, page = 1): ##### help
        self.add(ctx, 'help')
        helpers = [
            "AA Paceman extension: ?statscommands -> List of stats commands with details (WIP), "
            "?all -> List of all commands (no details) "
            "?help 2 -> Configuration/Setup, ?help 3 -> List of splits, ?help 4 -> Metainformation",
            "(help 2) ?join -> Join the bot to your channel, ?setplayer -> Set the default player"
            "for this channel, ?aapaceman -> Show aa-paceman setup link",
            f"(help 3) All splits: {', '.join(ALL_SPLITS)}",
            '(help 4) ?info -> Metadata on bot status, ?botdiscord -> Server with bot information, ?about -> Credits'
        ]
        p = page - 1
        if p < 0 or p >= len(helpers):
            return await do_send(ctx, f"Page number is out of bounds (maximum: {len(helpers)})")
        await do_send(ctx, helpers[p])
    @commands.command()
    async def statscommands(self, ctx: commands.Context): ##### help
        helpers = ["?average [splitname] [player] -> average split for a player, ?conversion "
                "[split1] [split2] [player] -> % of split1s that turn into split2s, ?countlt "
                "[split] [time] [player] -> Count the # of splits that are faster than [time]"]
        await do_send(ctx, helpers[0])
    @commands.command()
    async def all(self, ctx: commands.Context): ##### help
        await do_send(ctx, "?average, ?conversion, ?count, ?countlt, ?countgt, ?bastion_breakdown, ?latest, ?trend")
    @commands.command()
    async def aapaceman(self, ctx: commands.Context): ##### help
        await do_send(ctx, "AACord message link: "
                "https://discord.com/channels/835893596992438372/835893596992438375/1330305232516677733"
                " (how to set up aa paceman)")
    @commands.command()
    async def botdiscord(self, ctx: commands.Context): ##### bot discord
        self.add(ctx, 'botdiscord')
        await do_send(ctx, "For to-do list & feature requests: https://discord.gg/NSp5t3wfBP")
    @commands.command()
    async def about(self, ctx: commands.Context): ##### about
        self.add(ctx, 'about')
        await do_send(ctx, "Made by DesktopFolder. Uses stats from Jojoe's Paceman AA API. Uses local caching to reduce API calls.")
    @commands.command()
    async def info(self, ctx: commands.Context):  ##### info
        self.add(ctx, 'info')
        dur0 = duration_since_update()
        data = DATA_SORTED()
        dur = duration_since_update()
        infos = [f'Time since update: {dur}.']
        if dur0 != dur:
            infos.append(f'({dur0} before this command)')
        infos.append(f'Bot is in {len(self.configuration)} channels.')
        infos.append(f'{len(data)} known AA runs.')
        last_nether = PacemanObject(data[0])
        if last_nether.get('nether') is not None:
            infos.append(f'Latest nether: {last_nether.get_str("nether")} by {last_nether.player}.')
        tot_calls = 0
        stats_commands = {'average', 'conversion', 'count', 'countlt', 'countgt', 'bastion_breakdown', 'latest', 'trend', 'pb'}
        stats_stats = [f'command_{s}' for s in stats_commands]
        for v in self.configuration.values():
            for st in stats_stats:
                if st in v:
                    tot_calls += v[st]
        infos.append(f'~{tot_calls} total statistics queries made.')
        await do_send(ctx, ' '.join(infos))

    ########################################################################################
    ############################# Methods to configure the bot #############################
    ########################################################################################
    @commands.command()
    async def setplayer(self, ctx: commands.Context, playername: str):
        self.add(ctx, 'setplayer')
        if not isinstance(ctx.author, Chatter):
            return await do_send(ctx, 'Cannot validate that you are the broadcaster.')
        if not ctx.author.is_broadcaster:
            return await do_send(ctx, 'Only the broadcaster can use this command.')
        cn = ctx.channel.name.lower()
        if not cn in self.configuration:
            return await do_send(ctx, 'Let me know if you see this.')
        self.configuration[cn]['player'] = clean(playername)
        self.save()
        return await do_send(ctx, f'Set default player to {playername}.')

    @commands.command()
    async def join(self, ctx: commands.Context, agree: str = ""):
        self.add(ctx, 'join')
        cn = ctx.author.name
        if cn is None:
            return await do_send(ctx, "Name was none; if this issue persists, contact DesktopFolder.")
        if cn in self.configuration:
            return await do_send(ctx, f"Bot is already joined to {cn}.")
        cn = cn.lower()
        if agree != "agree":
            return await do_send(ctx, f'Notice: This is in development. See {self.prefix}botdiscord for current todos/feature requests. If you are okay with intermittent downtime & potential bugs, and want to join this bot to your channel ({cn}), type {self.prefix}join agree')
        self.configuration[cn] = {"name": cn}
        self.save()
        await self.join_channels([cn])
        return await do_send(ctx, f'Theoretically joined {cn}. Note: If you have follower mode chat limitations, you MUST mod FolderBot for it to work in your channel.')

    ########################################################################################
    ############################# Methods for stat querying :) #############################
    ########################################################################################
    async def parse(self, ctx: commands.Context, *args: str, default_split: Optional[str] = None, default_all=False):
        split: Optional[str] = None
        player: Optional[str] = None
        time_range: Optional[td] = None
        for arg in args:
            if arg.lower() in ALL_SPLITS:
                if split is not None:
                    await do_send(ctx, f'Argument {arg} provided (parsed as split) but {split} was already provided!')
                    return None
                split = arg.lower()
                continue
            tdp = td.try_parse(arg.lower())
            if tdp is not None:
                if time_range is not None:
                    await do_send(ctx, f'Argument {arg} provided (parsed as time range) but {time_range} was already provided!')
                    return None
                time_range = tdp
                continue
            # It should be a valid player.
            if arg != '!total' and clean(arg) != arg:
                await do_send(ctx, f'Argument {arg} provided (parsed as player) but is not a valid player.')
                return None
            if player is not None:
                await do_send(ctx, f'Argument {arg} provided (parsed as player) but {player} was already provided!')
                return None
            player = self.playername(ctx, arg)
        if player is None: # Always default player to the streamer
            if default_all:
                player = '!total'
            else:
                player = self.playername(ctx, None)
        if split is None and default_split is not None:
            split = default_split
        return ParseResult(split, player, time_range)

    async def parse_get(self, ctx: commands.Context, *args: str, default_split: Optional[str] = None, default_all=False):
        pr = await self.parse(ctx, *args, default_split=default_split, default_all=default_all)
        if pr is not None:
            return (pr, await pr.with_data(ctx))
        return (None, None)

    @commands.command()
    async def test_parse(self, ctx: commands.Context, *args: str):
        pr = await self.parse(ctx, *args)
        if pr is None:
            return # Failed parse.
        return await do_send(ctx, f'Parsed player as {pr.player_str()}, time range as {pr.time}, and split as {pr.split_str()}')
            
    @commands.command()
    async def average(self, ctx: commands.Context, *args: str):
        self.add(ctx, 'average')
        pr, pcs = await self.parse_get(ctx, *args)
        if pr is None or pcs is None:
            return
        #if not splitname in ALL_SPLITS:
        #    return await do_send(ctx, f'{splitname} is not a valid AA split: {ALL_SPLITS}')
        await do_send(ctx, f'Average AA {pr.split_str()} for {pr.player_str()}: {td.average(ts=[pc.always_get(pr.split_str()) for pc in pcs])} (sample: {len(pcs)}){pr.tr_str()}')

    @commands.command()
    async def conversion(self, ctx: commands.Context, split1: str, split2: str, *args: str):
        self.add(ctx, 'conversion')
        pr = await self.parse(ctx, *args)
        if pr is None:
            return
        if pr.split is not None:
            return await do_send(ctx, f'Found third split {pr.split} - likely parse failure.')
        # yikes need to do some refactoring
        split1 = split1.lower()
        split2 = split2.lower()
        for split in [split1, split2]:
            if not split in ALL_SPLITS:
                return await do_send(ctx, f'{split} is not a valid AA split: {ALL_SPLITS}')

        data = await pr.with_data(ctx)
        if data is None:
            return
        pcs = [p for p in data if p.filter(split=split1)]
        if len(pcs) == 0:
            return await do_send(ctx, f'{pr.player_str()} has no known {split1} AA splits.')
        n = len(pcs)
        x = len([p for p in pcs if p.has(split2)])
        await do_send(ctx, f'{pctg(n, x)}% ({x} / {n}) of {pr.player_str()}\'s AA {split1} splits lead to starting {split2} splits.{pr.tr_str()}')

    @commands.command()
    async def count(self, ctx: commands.Context, *args):
        self.add(ctx, 'count')
        pr, pcs = await self.parse_get(ctx, *args)
        if pr is None or pcs is None:
            return
        try:
            d = sorted(pcs, key=lambda p: p.always_get(pr.split_str()))
        except Exception as e:
            print(e)
            return await do_send(ctx, f'Encountered exception - Please message DesktopFolder :)')

        fastest = d[0].always_get(pr.split_str())
        fastest_name = d[0].player
        seed = f'{len(pcs)} known {pr.split_str()} times. Fastest: {td(fastest)}'
        if pr.player == '!total':
            return await do_send(ctx, f'There are {seed} (by {fastest_name}){pr.tr_str()}')
        else:
            return await do_send(ctx, f'{pr.player} has {seed}{pr.tr_str()}')

    def data_filtered(self, ctx: commands.Context, split: Optional[str], playername: Optional[str] = None):
        if playername == None:
            src = USEFUL_DATA()
        else:
            src = [p for p in USEFUL_DATA() if p.filter(player=playername)]
        src = [p for p in src if p.filter(split=split)]
        return src

    @commands.command()
    async def pb(self, ctx: commands.Context, *args: str):
        self.add(ctx, 'pb')
        pr, pcs = await self.parse_get(ctx, *args, default_split='finish')
        if pr is None or pcs is None:
            return

        try:
            d = sorted(pcs, key=lambda p: p.always_get(pr.split_str()))
        except Exception as e:
            print(e)
            return await do_send(ctx, f'Encountered exception - Please message DesktopFolder :)')
        
        fastest = d[0]
        ftime = btd(fastest.always_get(pr.split_str()))
        fplayer = d[0].player
        ftimesince = d[0].time_since() or "Unknown time"
        if isinstance(ftimesince, timedelta):
            ftimesince = btd(ftimesince)
        adder = '' if not pr.is_everyone() else f' (by {fplayer})'
        await do_send(ctx, f'Best known {pr.split_str()}{pr.tr_str()}: {ftime}{adder} ({ftimesince} ago)')


    @commands.command()
    async def lb(self, ctx: commands.Context, *args: str):
        self.add(ctx, 'lb')
        pr, pcs = await self.parse_get(ctx, *args, default_split='finish', default_all=True)
        if pr is None or pcs is None:
            return
        try:
            d = sorted(pcs, key=lambda p: p.always_get(pr.split_str()))[0:5]
        except Exception as e:
            print(e)
            return await do_send(ctx, f'Encountered exception - Please message DesktopFolder :)')

        times = [f'{i+1}. {btd(o.always_get(pr.split_str()))}' + (f' ({o.player})' if pr.is_everyone() else '') for i, o in enumerate(d)]

        await do_send(ctx, f'Top {len(times)} {pr.split_str()} times for {pr.player_str()}: ' + ', '.join(times))


    @commands.command()
    async def countlt(self, ctx: commands.Context, time: str, *args: str):
        self.add(ctx, 'countlt')
        try:
            maximum = td(time)
        except Exception:
            return await do_send(ctx, f'Invalid time {time}, follow format hh:mm:ss (hours/seconds optional, but seconds required for hours (note: countg/lt are now time followed by split, not split followed by time, sorry.')
        #if not split in ALL_SPLITS:
        #    return await do_send(ctx, f'{split} is not a valid AA split: {ALL_SPLITS}')
        pr, pcs = await self.parse_get(ctx, *args)
        if pr is None or pcs is None:
            return

        pcs = [t for t in [p.get(pr.split_str()) for p in pcs] if t is not None]
        pcs = [t for t in pcs if t <= maximum.src]

        if pr.player == '!total':
            return await do_send(ctx, f'There are {len(pcs)} known {pr.split_str()} times faster than {maximum}.{pr.tr_str()}')
        else:
            return await do_send(ctx, f'{pr.player_str()} has {len(pcs)} known {pr.split_str()} times faster than {maximum}.{pr.tr_str()}')

    @commands.command()
    async def countgt(self, ctx: commands.Context, time: str, *args: str):
        self.add(ctx, 'countgt')
        try:
            minimum = td(time)
        except Exception:
            return await do_send(ctx, f'Invalid time {time}, follow format hh:mm:ss (hours/seconds optional, but seconds required for hours')
        #if not split in ALL_SPLITS:
        #    return await do_send(ctx, f'{split} is not a valid AA split: {ALL_SPLITS}')
        pr, pcs = await self.parse_get(ctx, *args)
        if pr is None or pcs is None:
            return

        pcs = [t for t in [p.get(pr.split_str()) for p in pcs] if t is not None]
        pcs = [t for t in pcs if t > minimum.src]

        if pr.player == '!total':
            return await do_send(ctx, f'There are {len(pcs)} known {pr.split_str()} times slower than {minimum}.{pr.tr_str()}')
        else:
            return await do_send(ctx, f'{pr.player_str()} has {len(pcs)} known {pr.split_str()} times slower than {minimum}.{pr.tr_str()}')

    def playername(self, ctx: commands.Context, playername: Optional[str] = None) -> str:
        if playername and playername.strip() == '!total':
            # Special case
            return playername.strip()
        if playername is not None:
            return clean(playername)
        cn = ctx.channel.name.lower()
        if cn not in self.configuration:
            return 'If you see this, please tell DesktopFolder'
        c = self.configuration[cn]
        if 'player' in c:
            return c['player']
        return cn

    @commands.command()
    async def latest(self, ctx: commands.Context, *args):
        # TODO - n parameter
        self.add(ctx, 'latest')
        pr, pcs = await self.parse_get(ctx, *args)
        if pr is None or pcs is None:
            return
        lat = pcs[0].all_sorted()
        sz = pcs[0].time_since()
        if sz is not None:
            sz = str(sz)
            sz = sz.rsplit(':', maxsplit=1)[0]
            adder = f' ({sz} ago)'
        else:
            adder = ''
        return await do_send(ctx, f'Latest {pr.split_str()}{pr.tr_str()} for {pr.player_str()}: ' + ', '.join([f'{s}: {td(t)}' for s, t in lat]) + adder)

    @commands.command()
    async def trend(self, ctx: commands.Context, *args):
        from datetime import timedelta
        # TODO - n parameter
        self.add(ctx, 'trend')
        pr, pcs = await self.parse_get(ctx, *args)
        if pr is None or pcs is None:
            return
        # LATEST TO NOT LATEST
        d = [y for y in [x.get(pr.split_str()) for x in pcs] if y is not None]
        at = td.average(d) # all time average
        ld = len(d)
        # we'll take the latest 50, or the latest 1/3, whichever is SMALLER.
        num = min((ld//3), 50)
        newest = td.average(d[0:num])
        if newest == -1 or at == -1:
            return await do_send(ctx, f'Odd error, sorry eh.')
        diff = newest.src - at.src

        root = f"All-time average {pr.split_str()} split{pr.tr_str()} for {pr.player_str()} is {at} (sample: {ld}). Last {num} average is {newest}. "
        if diff > timedelta(seconds=0):
            # slower
            root += f'That is roughly {td(diff)} slower.'
        else:
            diff = diff * -1
            # faster :)
            root += f'That is roughly {td(diff)} faster, nice!'
        
        return await do_send(ctx, root)

    @commands.command()
    async def bastion_breakdown(self, ctx: commands.Context, *args):
        self.add(ctx, 'bastion_breakdown')
        pr, pcs = await self.parse_get(ctx, *args)
        if pr is None or pcs is None:
            return

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
        await do_send(ctx, f'Bastion conversion breakdown for {pr.player_str()}{pr.tr_str()}: {brk}')

    ########################################################################################
    ############################# Methods for AA leaderboard ###############################
    ########################################################################################
    @commands.command()
    async def aalb(self, ctx: commands.Context, *args):
        from daemon import localfile
        from requests import get 
        import re
        self.add(ctx, 'aalb')
        if len(args) <= 1:
            board = 'rsg'
            search_term = args[0].strip() if len(args) > 0 else self.playername(ctx)
        elif len(args) == 2:
            board = args[0]
            search_term = args[1].strip()
        else:
            return await do_send(ctx, 'Invalid number of arguments. Usage: ?aalb [board] <search>')
        uri = open(localfile("bad_aalb")).read().strip()
        board = re.sub(r'[^a-zA-Z0-9\.]', '_', board).lstrip('.')

        if search_term.isdigit():
            search_type = 'place'
        elif ':' in search_term:
            if search_term.startswith('<'):
                search_type = 'ltetime'
                search_term = search_term[1:]
            elif search_term.startswith('>'):
                search_type = 'gtetime'
                search_term = search_term[1:]
            else:
                search_type = 'gtetime'
            if search_term.count(':') == 1: # API expects HH:mm:ss but we want users to be able to use HH:mm
                search_term += ':00'
        else:
            search_type = 'name'
        uri = uri.format(BOARD=board, FILTER_TYPE=search_type, FILTER_VALUE=search_term)
        try:
            res = get(uri)
            if res.status_code != 200:
                return await do_send(ctx, 'Unable to fetch the leaderboard.')
            data = res.json()
            if 'results' not in data or not data['results']:
                return await do_send(ctx, 'No results found.')
            first_result = data['results'][0].get('run', {})
            place = first_result.get('place', '?')
            board = data.get('board', board)
            players = ', '.join([player for player in first_result.get('players', [])])
            completion_time = first_result.get('completionTime', '∞')
            return await do_send(ctx, f'{board} #{place}: {players} ({completion_time})')
        except Exception:
            return await do_send(ctx, 'Unable to fetch the leaderboard.')


if __name__ == '__main__':
    args = argv[1:]
    if 'test' in args:
        bot = Bot(prefix='%')
    else:
        bot = Bot()
    bot.run()
