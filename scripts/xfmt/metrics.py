import struct, sys
def tables(data):
    n = struct.unpack('>H', data[4:6])[0]
    t = {}
    for i in range(n):
        tag, _, off, ln = struct.unpack('>4sIII', data[12+16*i:28+16*i])
        t[tag.decode('latin1')] = (off, ln)
    return t
for path in sys.argv[1:]:
    d = open(path, 'rb').read()
    t = tables(d)
    upm = struct.unpack('>H', d[t['head'][0]+18:t['head'][0]+20])[0]
    o = t['hhea'][0]
    asc, desc, gap = struct.unpack('>hhh', d[o+4:o+10])
    o = t['OS/2'][0]
    fs = struct.unpack('>H', d[o+62:o+64])[0]
    ta, td, tg, wa, wd = struct.unpack('>hhhHH', d[o+68:o+78])
    win = wa + wd
    hh = asc - desc
    ext = max(0, gap - (win - hh))
    print(path.split('/')[-1], 'upm', upm, 'hhea', asc, desc, gap,
          'typo', ta, td, tg, 'win', wa, wd, 'useTypo', bool(fs & 128),
          'win/upm=%.4f' % (win/upm), 'hhea(a+d)/upm=%.4f' % (hh/upm),
          'ext=%d' % ext, 'f_writer=%.4f' % ((win+ext)/upm))
