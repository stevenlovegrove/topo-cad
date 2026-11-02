// The Cybertruck model's proportions (examples/cybertruck.ts), as the present pipeline's fit to two photos found them
// (old-pipeline: its fit.json; each mm at the model's scale). The model's own fit (cybertruck.fit.ts) is checked
// against these (topo-script's tests).
export const proportions = {
  bodyLow: 18.85136655903884, // the body's lower edge
  tailTop: 51.69792568065024, // the tail's top: the sails' points
  peakAlong: 114.14058749739111, // the roof's peak, along the truck
  peakHigh: 66.4053354040802, // and high
  chamfer: 3.3530865321323917, // the front corners' chamfers, across
  barAlong: 195.71947988284492, // the light bar's top edge, along
  barHigh: 41.43414031713661, // and high
  beltBehind: 5.853341172111261, // where the beltline meets the hood, behind the front corner
  rearAxle: 41.46610201328941, // the rear axle, along
  archTop: 12.660535104433926, // each arch's top corners, each way of its axle
  archFoot: 28.57070846295811, // its feet, each way
  rearArch: 42.05195172175748, // the rear arch's top, high
  frontArch: 41.9579052487653, // the front arch's top, high
  frontArchGap: 6.1910543159202005, // the front arch's front top corner, behind the front corner
  frontArchHigh: 41.06169359055825, // and high
  cabRise: 0.3101779896702612, // the roof's back, over the line from the tail to the peak
  rockerLow: 12.014326417827998, // the rocker's lower edge
};
