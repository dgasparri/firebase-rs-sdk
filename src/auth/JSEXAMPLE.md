Initialize Firebase Authentication:


> import { initializeApp } from "firebase/app";
> import { getAuth } from "firebase/auth";
> 
> // TODO: Replace the following with your app's Firebase project configuration
> // See: https://firebase.google.com/docs/web/learn-more#config-object
> const firebaseConfig = {
>   // ...
> };
> 
> // Initialize Firebase
> const app = initializeApp(firebaseConfig);
> 
> 
> // Initialize Firebase Authentication and get a reference to the service
> const auth = getAuth(app);


Create a form that allows existing users to sign in using their email address and password. When a user completes the form, call the signInWithEmailAndPassword method:

> import { getAuth, signInWithEmailAndPassword } from "firebase/auth";
> 
> const auth = getAuth();
> signInWithEmailAndPassword(auth, email, password)
>   .then((userCredential) => {
>     // Signed in 
>     const user = userCredential.user;
>     // ...
>   })
>   .catch((error) => {
>     const errorCode = error.code;
>     const errorMessage = error.message;
>   });