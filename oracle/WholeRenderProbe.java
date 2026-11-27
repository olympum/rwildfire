import java.awt.image.BufferedImage;
import java.io.BufferedOutputStream;
import java.io.DataOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import javax.imageio.ImageIO;
import org.jwildfire.base.Prefs;
import org.jwildfire.create.tina.base.Flame;
import org.jwildfire.create.tina.base.raster.AbstractRaster;
import org.jwildfire.create.tina.base.raster.RasterCreator;
import org.jwildfire.create.tina.base.raster.RasterFloatInt;
import org.jwildfire.create.tina.io.FlameReader;
import org.jwildfire.create.tina.random.RandomGeneratorFactory;
import org.jwildfire.create.tina.random.RandomGeneratorType;
import org.jwildfire.create.tina.render.FlameRenderer;
import org.jwildfire.create.tina.render.RenderInfo;
import org.jwildfire.create.tina.render.RenderMode;
import org.jwildfire.create.tina.render.RenderedFlame;
import org.jwildfire.create.tina.base.raster.RasterPoint;

public final class WholeRenderProbe {
  public static void main(String[] args) throws Exception {
    int width = Integer.parseInt(args[2]), height = Integer.parseInt(args[3]);
    int quality = Integer.parseInt(args[4]);
    long seed = Long.parseLong(args[5]);
    Prefs prefs = Prefs.getPrefs();
    prefs.setTinaPreserveFreeCPUs(Integer.MAX_VALUE);
    prefs.setTinaRandomNumberGenerator(RandomGeneratorType.MARSAGLIA);
    prefs.setTinaRasterType(RasterCreator.FLOAT_INT_RASTER);
    if (prefs.getTinaRenderThreads() != 1) throw new AssertionError("expected one worker");
    Flame flame = new FlameReader(prefs).readFlames(args[0]).get(0);
    flame.setSampleDensity(quality);
    flame.setPixelsPerUnitScale((width / (double) flame.getWidth() + height / (double) flame.getHeight()) / 2.0);
    // Leave legacy density estimation out of this first still-render contract.
    flame.setDeRadius(0.0);
    flame.setAntialiasAmount(Double.parseDouble(args[6]));
    flame.setAntialiasRadius(Double.parseDouble(args[7]));
    if (flame.getSpatialOversampling() != Integer.parseInt(args[9]) || flame.getSpatialFilterRadius() != Double.parseDouble(args[10]) || flame.getLayers().size() != Integer.parseInt(args[8])) {
      throw new AssertionError("unexpected layer count or oversampling");
    }
    // Renderer context and its only worker both obtain the cached MARSAGLIA#0 instance.
    RandomGeneratorFactory.getInstance(prefs, RandomGeneratorType.MARSAGLIA, 0).randomize(seed);
    RenderInfo info = new RenderInfo(width, height, RenderMode.PRODUCTION);
    info.setStoreRaster(true);
    RenderedFlame result = new FlameRenderer(flame, prefs, false, false).renderFlame(info);
    AbstractRaster raster = result.getRaster();
    if (raster == null || raster.getClass() != RasterFloatInt.class) {
      throw new AssertionError("expected retained float/int raster");
    }
    RasterPoint bin = new RasterPoint(0);
    try (DataOutputStream out = output(args[1], ".raster")) {
      out.writeInt(raster.getRasterWidth());
      out.writeInt(raster.getRasterHeight());
      for (int y = 0; y < raster.getRasterHeight(); y++) {
        for (int x = 0; x < raster.getRasterWidth(); x++) {
          raster.readRasterPoint(x, y, bin);
          out.writeInt(Math.toIntExact(bin.count));
          out.writeFloat((float) bin.red);
          out.writeFloat((float) bin.green);
          out.writeFloat((float) bin.blue);
        }
      }
    }
    File png = new File(args[1] + ".png");
    if (! ImageIO.write(result.getImage().getBufferedImg(), "png", png)) {
      throw new AssertionError("no PNG writer");
    }
    BufferedImage image = ImageIO.read(png);
    if (image == null || image.getWidth() != width || image.getHeight() != height) {
      throw new AssertionError("invalid decoded PNG dimensions");
    }
    try (DataOutputStream out = output(args[1], ".rgb")) {
      for (int y = 0; y < height; y++) {
        for (int x = 0; x < width; x++) {
          int rgb = image.getRGB(x, y);
          out.writeByte(rgb >>> 16);
          out.writeByte(rgb >>> 8);
          out.writeByte(rgb);
        }
      }
    }
  }

  private static DataOutputStream output(String prefix, String suffix) throws Exception {
    return new DataOutputStream(new BufferedOutputStream(new FileOutputStream(prefix + suffix)));
  }
}
