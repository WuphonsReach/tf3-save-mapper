# Grades: examples

`tf3map SAVE --rail-grades` colours each track by its grade: the height between the edge's two ends over its length along the curve, up or down alike. At the default limit of 13 % (the steepest the game's track types allow) there is one colour per whole percent, and magenta from the limit up. `--road-grades` does the same for the roads (streets with tram lanes among them) and `--tram-grades` for the tram lines of their own, both with a default limit of 20 %, as steep as the game builds them. `--rail-grade-limit P`, `--road-grade-limit P` and `--tram-grade-limit P` spread the same 14 colours under another limit. The [README](../README.md#using-it) has the details.

Each map carries its key along the bottom: a row per family drawn, its 14 colours, with the grade at each gap between two of them.

Every map here is from a save published in the mod.io catalogue, drawn with the default vector terrain at 1 m per pixel. Click a map for the full size; in the full SVG each segment's grade is in its `data-grade` attribute. The maps are redrawn by [regenerate-examples.sh](regenerate-examples.sh).

<table>
<tr>
<td width="50%"><a href="images/rail-grades-helix.svg"><img src="images/rail-grades-helix.svg" width="420" alt="A helix of six rings of track, all orange at a steady 8 to 9 %, with a short red stretch where the line leaves it."></a><br>
<b>A helix</b> (mod.io 6414627): six rings climbing at a steady 8 to 9 %, a red stretch where the line leaves them, and the level lines through the town beside it.<br>
<code>--crop 4680,-1410,940</code></td>
<td width="50%"><a href="images/rail-grades-horseshoe.svg"><img src="images/rail-grades-horseshoe.svg" width="420" alt="A four-track line climbing round a horseshoe curve in orange and yellow, with dashed tunnels at both ends."></a><br>
<b>A horseshoe curve</b> (mod.io 6459692): a four-track mountain line climbing in orange round the curve, tunnels at both ends (dashed), and a lower line going red into its tunnel.<br>
<code>--crop -1500,0,940</code></td>
</tr>
<tr>
<td><a href="images/rail-grades-junction.svg"><img src="images/rail-grades-junction.svg" width="420" alt="A busy junction: a many-track bridge stepping from orange to green as it levels out, ramps up to overpasses, and dashed tunnels under the roads."></a><br>
<b>A junction</b> (mod.io 6487589): a many-track bridge stepping from orange to green as it levels out, ramps up to the overpasses, and tunnels under the roads.<br>
<code>--crop -1750,-5250,500</code></td>
<td><a href="images/rail-grades-fan.svg"><img src="images/rail-grades-fan.svg" width="420" alt="Seven lines fanning out of one bundle, several with red and orange climbs, the bundle stepping orange, yellow, green."></a><br>
<b>A fan</b> (mod.io 6433417): seven lines out of one bundle, several climbing in red and orange, the bundle stepping orange, yellow, green.<br>
<code>--crop 950,700,940</code></td>
</tr>
<tr>
<td><a href="images/rail-grades-town.svg"><img src="images/rail-grades-town.svg" width="420" alt="A harbour town with a multi-track line down its west side, its tunnel dashed, with red and yellow pulls into the station."></a><br>
<b>A harbour town</b> (mod.io 6448903): a multi-track line down the west side, its tunnel dashed, with red and yellow pulls on the way into the station.<br>
<code>--crop 0,-600,1000</code></td>
<td></td>
</tr>
</table>

Each is `tf3map SAVE --crop X,Y,R --mpp 1 --rail-grades`.

## Roads and tram lines

<table>
<tr>
<td width="50%"><a href="images/grades-hill-town.svg"><img src="images/grades-hill-town.svg" width="420" alt="A hilly town's street grid, level streets green, the streets up the slopes orange and red, a few magenta, with dashed road and tram tunnels under it."></a><br>
<b>A hill town</b> (mod.io 6431683): the level grid in green, the streets up the slopes orange to red and a few over 20 % in magenta, and the road and tram tunnels beneath, dashed.<br>
<code>--crop -1488,1446,700 --road-grades --tram-grades</code></td>
<td width="50%"><a href="images/grades-tram-climb.svg"><img src="images/grades-tram-climb.svg" width="420" alt="A tram line in a tunnel climbing from green through red to magenta, over 30 %, where it crosses under a rail line; a level road beside it."></a><br>
<b>A tram climb</b> (mod.io 6426294): a tram line in a tunnel climbing from green through red to magenta, past 30 %, as it crosses a rail line; the road beside it is level.<br>
<code>--crop 4576,-2485,930 --road-grades --tram-grades --rail-grades</code></td>
</tr>
</table>

Each is `tf3map SAVE --crop X,Y,R --mpp 1` and the options given.
