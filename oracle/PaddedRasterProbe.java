// SPDX-License-Identifier: GPL-3.0-or-later
import java.io.BufferedOutputStream;
import java.io.DataOutputStream;
import java.io.FileOutputStream;
import org.jwildfire.base.Prefs;
import org.jwildfire.create.tina.base.*;
import org.jwildfire.create.tina.base.raster.*;
import org.jwildfire.create.tina.io.FlameReader;
import org.jwildfire.create.tina.random.MarsagliaRandomGenerator;
import org.jwildfire.create.tina.render.*;
import org.jwildfire.create.tina.variation.FlameTransformationContext;

public class PaddedRasterProbe {
  public static void main(String[] args) throws Exception {
    Flame flame = new FlameReader(Prefs.getPrefs()).readFlames(args[0]).get(0);
    int width = 2 * flame.getWidth(), height = 2 * flame.getHeight();
    flame.setPixelsPerUnitScale(2.0);
    flame.setAntialiasAmount(0.0);
    int quality = 100;
    MarsagliaRandomGenerator rng = new MarsagliaRandomGenerator();
    rng.randomize(741103597L);
    int border = new LogDensityFilter(flame, rng).getBorderWidth();
    if (border != 2 || flame.getSpatialOversampling() != 1) throw new AssertionError("expected border 2 and oversampling 1");
    int stride = width + 2 * border, rows = height + 2 * border;
    long budget = (long) quality * stride * rows;
    Layer layer = flame.getFirstLayer();
    FlameTransformationContext ctx = new FlameTransformationContext(null, rng, 0, 0);
    layer.refreshModWeightTables(ctx);
    FlameRendererView view = new FlameRendererView(Stereo3dEye.UNSPECIFIED, flame, rng, border, width, height, stride, rows, ctx, null);
    XYZPoint p = new XYZPoint(), aff = new XYZPoint(), var = new XYZPoint();
    p.x = 2.0 * rng.random() - 1.0;
    p.y = 2.0 * rng.random() - 1.0;
    p.color = rng.random();
    p.material = rng.random();
    XForm current = layer.getXForms().get(0);
    current.transformPoint(ctx, aff, var, p, p);
    for (int i = 0; i <= Constants.INITIAL_ITERATIONS; i++) {
      current = current.getNextAppliedXFormTable()[rng.random(Constants.NEXT_APPLIED_XFORM_TABLE_SIZE)];
      current.transformPoint(ctx, aff, var, p, p);
    }
    RasterFloatInt raster = new RasterFloatInt();
    raster.allocRaster(flame, stride, rows, 1, quality);
    XYZProjectedPoint projected = new XYZProjectedPoint(0);
    PlotSample sample = new PlotSample();
    PlotSample[] samples = {sample};
    for (long i = 0; i < budget; i++) {
      current = current.getNextAppliedXFormTable()[rng.random(Constants.NEXT_APPLIED_XFORM_TABLE_SIZE)];
      current.transformPoint(ctx, aff, var, p, p);
      if (!view.project(p, projected)) continue;
      int x = (int) (view.getBws() * projected.x + 0.5);
      int y = (int) (view.getBhs() * projected.y + 0.5);
      if (x < 0 || x >= stride || y < 0 || y >= rows) continue;
      sample.screenX = x;
      sample.screenY = y;
      sample.r = p.redColor;
      sample.g = p.greenColor;
      sample.b = p.blueColor;
      raster.addSamples(samples, 1);
    }
    RasterPoint point = new RasterPoint(0);
    try (DataOutputStream out = new DataOutputStream(new BufferedOutputStream(new FileOutputStream(args[1])))) {
      out.writeInt(width);
      out.writeInt(height);
      out.writeInt(quality);
      for (int y = 0; y < rows; y++) {
        for (int x = 0; x < stride; x++) {
          raster.readRasterPoint(x, y, point);
          out.writeInt(Math.toIntExact(point.count));
          out.writeFloat((float) point.red);
          out.writeFloat((float) point.green);
          out.writeFloat((float) point.blue);
        }
      }
    }
  }
}
