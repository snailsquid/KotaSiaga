// Adds settings that must come from env, so no key is ever committed (AGENTS.md rule 7).
// GOOGLE_MAPS_ANDROID_API_KEY is only needed for Android builds outside Expo Go.
module.exports = ({ config }) => ({
  ...config,
  plugins: [
    ...(config.plugins ?? []),
    [
      'react-native-maps',
      {
        androidGoogleMapsApiKey: process.env.GOOGLE_MAPS_ANDROID_API_KEY,
        iosGoogleMapsApiKey: process.env.GOOGLE_MAPS_IOS_API_KEY,
      },
    ],
  ],
});
