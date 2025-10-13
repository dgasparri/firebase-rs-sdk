From: https://firebase.google.com/docs/app-check/web/recaptcha-provider

Add the following initialization code to your application, before you access any Firebase services. You will need to pass your reCAPTCHA site key, which you created in the reCAPTCHA console, to activate():

> import { initializeApp } from "firebase/app";
> import { initializeAppCheck, ReCaptchaV3Provider } from "firebase/app-check";
> 
> const app = initializeApp({
>   // Your firebase configuration object
> });
> 
> // Pass your reCAPTCHA v3 site key (public key) to activate(). Make sure this
> // key is the counterpart to the secret key you set in the Firebase console.
> const appCheck = initializeAppCheck(app, {
>   provider: new ReCaptchaV3Provider('abcdefghijklmnopqrstuvwxy-1234567890abcd'),
> 
>   // Optional argument. If true, the SDK automatically refreshes App Check
>   // tokens as needed.
>   isTokenAutoRefreshEnabled: true
> });