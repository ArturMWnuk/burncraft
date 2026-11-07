declare module "*.css";

declare module "*.ftl?raw" {
  const src: string;
  export default src;
}
